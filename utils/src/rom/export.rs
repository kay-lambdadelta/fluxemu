use std::{error::Error, path::PathBuf, sync::Arc};

use fluxemu_database::{FileIdentification, ProgramDatabase};
use fluxemu_environment::Environment;
use fluxemu_program::{FileDigest, RomId, RomStoreSpecification};
use futures::{StreamExt, TryStreamExt, channel::mpsc};
use tokio::{fs, task::spawn_blocking};

use crate::{cli::ExportStyle, concurrency_limit, rom::symlink_file};

struct ExportContext {
    program_database: Arc<ProgramDatabase>,
    destination_path: PathBuf,
    symlink: bool,
    style: ExportStyle,
}

pub async fn export(
    program_database: Arc<ProgramDatabase>,
    environment: &Environment,
    destination_path: PathBuf,
    symlink: bool,
    style: ExportStyle,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    let _ = fs::create_dir_all(&destination_path).await;

    let rom_stores: Vec<PathBuf> = environment
        .rom_stores
        .values()
        .filter_map(RomStoreSpecification::filesystem_root)
        .collect();

    let context = Arc::new(ExportContext {
        program_database,
        destination_path,
        symlink,
        style,
    });

    let (sender, receiver) = mpsc::unbounded();

    let store_reader = spawn_blocking(move || {
        for rom_store in rom_stores {
            let Ok(entries) = std::fs::read_dir(&rom_store) else {
                continue;
            };

            for entry in entries.flatten() {
                let path = entry.path();

                let Some(rom_id) = path
                    .file_name()
                    .and_then(|file_name| file_name.to_str())
                    .and_then(|file_name| file_name.parse::<RomId>().ok())
                else {
                    continue;
                };

                if sender.unbounded_send((rom_id, path)).is_err() {
                    return;
                }
            }
        }
    });

    receiver
        .map(Ok::<_, Box<dyn Error + Send + Sync>>)
        .try_for_each_concurrent(concurrency_limit(), move |(rom_id, source_rom_path)| {
            let context = context.clone();

            async move {
                export_rom(&context, rom_id, source_rom_path).await;

                Ok(())
            }
        })
        .await?;

    store_reader.await?;

    Ok(())
}

async fn export_rom(context: &ExportContext, rom_id: RomId, source_rom_path: PathBuf) {
    let identifications = spawn_blocking({
        let program_database = context.program_database.clone();
        let source_rom_path = source_rom_path.clone();

        move || -> Result<Vec<FileIdentification>, Box<dyn Error + Send + Sync>> {
            let digest = FileDigest::new(std::fs::File::open(source_rom_path)?)?;

            Ok(program_database.identify_file(&digest)?)
        }
    })
    .await;

    let identifications = match identifications {
        Ok(Ok(identifications)) => identifications,
        Ok(Err(err)) => {
            tracing::error!("Could not identify ROM {}: {}", rom_id, err);
            return;
        }
        Err(err) => {
            tracing::error!("Could not identify ROM {}: {}", rom_id, err);
            return;
        }
    };

    let destination_path = &context.destination_path;

    for identification in identifications {
        let destination_rom_path = match context.style {
            ExportStyle::NoIntro => {
                let machine_folder =
                    destination_path.join(identification.program_id.system.to_nointro_string());
                let program_folder = machine_folder.join(&identification.program_id.main_name);

                program_folder.join(&identification.path)
            }
            ExportStyle::Native => destination_path.join(rom_id.to_string()),
            ExportStyle::EmulationStation => todo!(),
        };

        if !destination_rom_path.starts_with(destination_path) {
            tracing::error!("Export path is outside of the target directory");

            continue;
        }

        if let Some(parent) = destination_rom_path.parent() {
            let _ = fs::create_dir_all(parent).await;
        }

        tracing::info!("Exporting ROM for program {}", identification.program_id);

        let result = match identification
            .file
            .missing_header(identification.file_match.form)
        {
            Some(header) => match fs::read(&source_rom_path).await {
                Ok(contents) => {
                    fs::write(&destination_rom_path, [header.as_ref(), &contents].concat()).await
                }
                Err(err) => Err(err),
            },
            None if context.symlink => symlink_file(&source_rom_path, &destination_rom_path).await,
            None => fs::copy(&source_rom_path, &destination_rom_path)
                .await
                .map(|_| ()),
        };

        if let Err(err) = result {
            tracing::error!(
                "Could not output ROM to path {}: {}",
                destination_rom_path.display(),
                err
            );
        }
    }
}
