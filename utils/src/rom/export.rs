use std::{error::Error, path::PathBuf, sync::Arc};

use fluxemu_database::{PROGRAM_INFORMATION_TABLE, ProgramDatabase};
use fluxemu_environment::Environment;
use fluxemu_program::{ProgramId, ProgramInfo, RomId, RomStoreSpecification};
use redb::{ReadableDatabase, ReadableMultimapTable};
use tokio::{fs, sync::mpsc, task::spawn_blocking};

use crate::{cli::ExportStyle, for_each_concurrent, rom::symlink_file};

struct ExportContext {
    rom_stores: Vec<PathBuf>,
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

    let context = Arc::new(ExportContext {
        rom_stores: environment
            .rom_stores
            .values()
            .filter_map(RomStoreSpecification::filesystem_root)
            .collect(),
        destination_path,
        symlink,
        style,
    });

    let (sender, receiver) = mpsc::unbounded_channel();

    let table_reader = spawn_blocking(move || -> Result<(), Box<dyn Error + Send + Sync>> {
        let read_transaction = program_database.database().begin_read()?;
        let program_information_table =
            read_transaction.open_multimap_table(PROGRAM_INFORMATION_TABLE)?;

        for (id, info_values) in program_information_table.iter()?.flatten() {
            let id = id.value();

            for info in info_values.into_iter().flatten() {
                if sender.send((id.clone(), info.value())).is_err() {
                    return Ok(());
                }
            }
        }

        Ok(())
    });

    for_each_concurrent(receiver, move |(id, info)| {
        let context = context.clone();

        async move {
            export_program(&context, id, info).await;

            Ok(())
        }
    })
    .await?;

    table_reader.await?
}

async fn find_rom(rom_stores: &[PathBuf], rom_id: RomId) -> Option<PathBuf> {
    for store in rom_stores {
        let rom_path = store.join(rom_id.to_string());

        if fs::try_exists(&rom_path).await.unwrap_or(false) {
            return Some(rom_path);
        }
    }

    None
}

async fn export_program(context: &ExportContext, id: ProgramId, info: ProgramInfo) {
    let destination_path = &context.destination_path;

    for (rom_id, file_names) in info.filesystem() {
        let rom_id = *rom_id;

        for file_name in file_names {
            let Some(source_rom_path) = find_rom(&context.rom_stores, rom_id).await else {
                continue;
            };

            let destination_rom_path = match context.style {
                ExportStyle::NoIntro => {
                    let machine_folder_name = id.system.to_nointro_string();
                    let machine_folder = destination_path.join(machine_folder_name);
                    let program_folder = machine_folder.join(&id.main_name);
                    let final_path = program_folder.join(file_name);

                    let _ = fs::create_dir_all(final_path.parent().unwrap()).await;

                    final_path
                }
                ExportStyle::Native => destination_path.join(rom_id.to_string()),
                ExportStyle::EmulationStation => todo!(),
            };

            if !destination_rom_path.starts_with(destination_path) {
                tracing::error!("Export path is outside of the target directory");

                continue;
            }

            tracing::info!("Exporting ROM for program {}", id);

            let result = if context.symlink {
                symlink_file(&source_rom_path, &destination_rom_path).await
            } else {
                fs::copy(&source_rom_path, &destination_rom_path)
                    .await
                    .map(|_| ())
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
}
