use std::{error::Error, path::PathBuf, process::ExitCode, sync::Arc};

use clap::Parser;
use fluxemu_environment::load_environment;
use fluxemu_program::{ProgramManager, SystemId};
use rayon::iter::{IntoParallelIterator, ParallelBridge, ParallelIterator};
use redb::Database;
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
    let environment = load_environment();

    let filter = Arc::new(
        EnvFilter::builder()
            .with_regex(true)
            .with_default_directive(LevelFilter::INFO.into())
            .from_env_lossy(),
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

    let program_manager = ProgramManager::new(
        Database::create(&environment.database_location)?,
        environment.rom_store_directories.clone(),
    )?;

    match args {
        Cli::Database(command) => match command {
            DatabaseCommand::Import { paths } => {
                database::native::import(&program_manager, walk_for_files(paths))?;
            }
        },
        Cli::Rom(command) => match command {
            RomCommand::Import { paths, symlink } => {
                rom::import(
                    &program_manager,
                    &environment.rom_store_directories[0],
                    symlink,
                    walk_for_files(paths),
                )?;
            }
            RomCommand::Export {
                symlink,
                style,
                destination,
            } => {
                rom::export(&program_manager, &environment, destination, symlink, style)?;
            }
            RomCommand::Patch {
                source,
                patch,
                add_to_database,
            } => {
                let source = source.to_id(&program_manager)?;

                rom::patch(
                    &program_manager,
                    &environment,
                    source,
                    &patch,
                    add_to_database,
                )?;
            }
            RomCommand::Verify => {}
        },
        Cli::Manifest(command) => match command {
            ManifestCommand::Import { paths } => {
                manifest::import(&program_manager, walk_for_files(paths))?;
            }
            ManifestCommand::Export { output_directory } => {
                manifest::export(&program_manager, &output_directory)?;
            }
        },
        Cli::Logiqx(command) => match command {
            LogiqxCommand::Import { paths } => {
                logiqx::import(&program_manager, walk_for_files(paths))?;
            }
            LogiqxCommand::DownloadRedump { system_filter } => {
                if system_filter.is_empty() {
                    for machine_id in SystemId::iter() {
                        if !system_filter.contains(&machine_id)
                            && let Ok(redump_system) = RedumpSystem::try_from(machine_id)
                        {
                            download_and_import_redump_system(&program_manager, redump_system)?;
                        }
                    }
                } else {
                    for machine_id in system_filter {
                        if let Ok(redump_system) = RedumpSystem::try_from(machine_id) {
                            download_and_import_redump_system(&program_manager, redump_system)?;
                        }
                    }
                }
            }
        },
    }

    Ok(())
}

/// Turns the paths into a walkdir that automatically walks through links and filters to files
#[inline]
fn walk_for_files(
    paths: impl IntoParallelIterator<Item = PathBuf>,
) -> impl ParallelIterator<Item = Result<walkdir::DirEntry, walkdir::Error>> {
    paths.into_par_iter().flat_map(|path| {
        let walkdir = WalkDir::new(path).follow_links(true);

        walkdir
            .into_iter()
            .par_bridge()
            .filter(|entry| entry.as_ref().is_ok_and(|entry| entry.path().is_file()))
    })
}
