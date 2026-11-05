use std::{
    error::Error, num::NonZero, path::PathBuf, process::ExitCode, sync::Arc,
    thread::available_parallelism,
};

use clap::Parser;
use fluxemu_database::ProgramDatabase;
use fluxemu_environment::load_environment;
use fluxemu_program::{ProgramManager, RomStoreSpecification, SystemId};
use redb::Database;
use tokio::{
    runtime::Runtime,
    sync::mpsc,
    task::{JoinSet, spawn_blocking},
};
use tracing::level_filters::LevelFilter;
use tracing_subscriber::{
    EnvFilter, Layer,
    fmt::format::FmtSpan,
    layer::{Filter, SubscriberExt},
    util::SubscriberInitExt,
};
use walkdir::WalkDir;

use crate::{
    cli::{Cli, DatabaseCommand, LogiqxCommand, ManifestCommand, RomCommand},
    database::redump::{RedumpSystem, download_and_import_redump_system},
};

mod cli;
mod database;
mod logiqx;
mod manifest;
mod rom;

fn main() -> ExitCode {
    match run() {
        Ok(_) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("{}", err);
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<(), Box<dyn Error + Send + Sync>> {
    Runtime::new()?.block_on(async {
        let environment = load_environment();

        let filter = Arc::new(
            EnvFilter::builder()
                .with_regex(true)
                .with_default_directive(LevelFilter::INFO.into())
                .from_env_lossy()
                .add_directive("nod=warn".parse()?),
        );
        let stderr_layer = tracing_subscriber::fmt::layer()
            .with_writer(std::io::stderr)
            .with_ansi(true)
            .with_span_events(FmtSpan::CLOSE)
            .with_thread_names(true)
            .with_thread_ids(false);
        tracing_subscriber::registry()
            .with(stderr_layer.with_filter(filter.clone() as Arc<dyn Filter<_> + Send + Sync>))
            .init();

        let args = Cli::parse();

        let program_database =
            ProgramDatabase::new(Database::create(&environment.database_location)?)?;
        let program_manager = ProgramManager::new(environment.rom_stores.clone())?;

        match args {
            Cli::Database(command) => match command {
                DatabaseCommand::Import { paths } => {
                    database::native::import(program_database, walk_for_files(paths)).await?;
                }
            },
            Cli::Rom(command) => match command {
                RomCommand::Import { paths, symlink } => {
                    let rom_store = environment
                        .rom_stores
                        .values()
                        .find_map(RomStoreSpecification::filesystem_root)
                        .ok_or("No ROM store is a local directory")?;

                    rom::import(&program_database, rom_store, symlink, walk_for_files(paths))
                        .await?;
                }
                RomCommand::Export {
                    symlink,
                    style,
                    destination,
                } => {
                    rom::export(program_database, &environment, destination, symlink, style)
                        .await?;
                }
                RomCommand::Patch {
                    source,
                    patch,
                    add_to_database,
                } => {
                    let source = source.to_id(&program_manager).await?;

                    rom::patch(
                        &program_manager,
                        program_database,
                        &environment,
                        source,
                        &patch,
                        add_to_database,
                    )
                    .await?;
                }
                RomCommand::Verify => {}
            },
            Cli::Manifest(command) => match command {
                ManifestCommand::Import { paths } => {
                    manifest::import(program_database, walk_for_files(paths)).await?;
                }
                ManifestCommand::Export { output_directory } => {
                    manifest::export(program_database, output_directory).await?;
                }
            },
            Cli::Logiqx(command) => match command {
                LogiqxCommand::Import { paths } => {
                    logiqx::import(program_database, walk_for_files(paths)).await?;
                }
                LogiqxCommand::DownloadRedump { system_filter } => {
                    let client = reqwest::Client::new();

                    if system_filter.is_empty() {
                        for machine_id in SystemId::iter() {
                            if !system_filter.contains(&machine_id)
                                && let Ok(redump_system) = RedumpSystem::try_from(machine_id)
                            {
                                download_and_import_redump_system(
                                    &client,
                                    program_database.clone(),
                                    redump_system,
                                )
                                .await?;
                            }
                        }
                    } else {
                        for machine_id in system_filter {
                            if let Ok(redump_system) = RedumpSystem::try_from(machine_id) {
                                download_and_import_redump_system(
                                    &client,
                                    program_database.clone(),
                                    redump_system,
                                )
                                .await?;
                            }
                        }
                    }
                }
            },
        }

        Ok(())
    })
}

/// Turns the paths into a walkdir that automatically walks through links and filters to files
fn walk_for_files(paths: Vec<PathBuf>) -> mpsc::UnboundedReceiver<walkdir::DirEntry> {
    let (sender, receiver) = mpsc::unbounded_channel();

    spawn_blocking(move || {
        for entry in paths
            .into_iter()
            .flat_map(|path| WalkDir::new(path).follow_links(true))
        {
            match entry {
                Ok(entry) if entry.path().is_file() => {
                    if sender.send(entry).is_err() {
                        return;
                    }
                }
                Ok(_) => {}
                Err(err) => {
                    tracing::error!("Failed to open file: {}", err);
                }
            }
        }
    });

    receiver
}

async fn for_each_concurrent<
    T,
    F: Future<Output = Result<(), Box<dyn Error + Send + Sync>>> + Send + 'static,
>(
    mut receiver: mpsc::UnboundedReceiver<T>,
    action: impl Fn(T) -> F,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    let limit = available_parallelism().map_or(1, NonZero::get);
    let mut tasks = JoinSet::new();

    while let Some(item) = receiver.recv().await {
        if tasks.len() >= limit
            && let Some(result) = tasks.join_next().await
        {
            result??;
        }

        tasks.spawn(action(item));
    }

    while let Some(result) = tasks.join_next().await {
        result??;
    }

    Ok(())
}
