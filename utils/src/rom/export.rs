use std::{error::Error, fs::create_dir_all, path::PathBuf};

use fluxemu_environment::Environment;
use fluxemu_program::{PROGRAM_INFORMATION_TABLE, ProgramManager};
use rayon::iter::{IntoParallelRefIterator, ParallelBridge, ParallelIterator};
use redb::{ReadableDatabase, ReadableMultimapTable};

use crate::cli::ExportStyle;

pub fn export(
    program_manager: &ProgramManager,
    environment: &Environment,
    destination_path: PathBuf,
    symlink: bool,
    style: ExportStyle,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    let _ = create_dir_all(&destination_path);

    let read_transaction = program_manager.database().begin_read()?;
    let program_information_table =
        read_transaction.open_multimap_table(PROGRAM_INFORMATION_TABLE)?;

    program_information_table
        .iter()?
        .par_bridge()
        .flatten()
        .flat_map(|(id, info_values)| {
            let id = id.value();

            info_values
                .into_iter()
                .par_bridge()
                .flatten()
                .map(move |info| (id.clone(), info))
        })
        .try_for_each(|(id, info)| {
            let info = info.value();

            for (rom_id, file_name) in info.filesystem().iter().flat_map(|(rom_id, file_names)| {
                file_names.iter().map(|file_name| (*rom_id, file_name))
            }) {
                let Some(source_rom_path) = environment
                    .rom_store_directories
                    .par_iter()
                    .map(|store| store.join(rom_id.to_string()))
                    .find_first(|rom_path| rom_path.exists())
                else {
                    continue;
                };

                let destination_rom_path = match style {
                    ExportStyle::NoIntro => {
                        let machine_folder_name = id.system.to_nointro_string();
                        let machine_folder = destination_path.join(machine_folder_name);
                        let program_folder = machine_folder.join(&id.main_name);
                        let final_path = program_folder.join(file_name);

                        let _ = create_dir_all(final_path.parent().unwrap());

                        final_path
                    }
                    ExportStyle::Native => destination_path.join(rom_id.to_string()),
                    ExportStyle::EmulationStation => todo!(),
                };

                if !destination_rom_path.starts_with(&destination_path) {
                    tracing::error!("Export path is outside of the target directory");

                    continue;
                }

                tracing::info!("Exporting ROM for program {}", id);

                if symlink {
                    if let Err(err) = cfg_select! {
                        target_family = "unix" => {
                            std::os::unix::fs::symlink(source_rom_path, &destination_rom_path)
                        }
                        target_os = "windows" => std::os::windows::fs::symlink_file(
                            source_rom_path,
                            &destination_rom_path,
                        ),
                        _ => {
                            panic!("Unsupported operating system for symlinking")
                        }
                    } {
                        tracing::error!(
                            "Could not output ROM to path {}: {}",
                            destination_rom_path.display(),
                            err
                        );
                    }
                } else {
                    if let Err(err) = std::fs::copy(source_rom_path, &destination_rom_path) {
                        tracing::error!(
                            "Could not output ROM to path {}: {}",
                            destination_rom_path.display(),
                            err
                        );
                    }
                }
            }

            Ok(())
        })
}
