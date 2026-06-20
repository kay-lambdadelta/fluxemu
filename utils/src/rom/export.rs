use std::{
    path::{Path, PathBuf},
    sync::Arc,
};

use fluxemu_program::{PROGRAM_INFORMATION_TABLE, ProgramManager};
use redb::{ReadableDatabase, ReadableMultimapTable};

use crate::ExportStyle;

pub fn rom_export(
    destination_path: PathBuf,
    program_manager: Arc<ProgramManager>,
    rom_store: &Path,
    symlink: bool,
    style: ExportStyle,
) {
    let _ = std::fs::create_dir_all(&destination_path);

    let read_transaction = match program_manager.database().begin_read() {
        Ok(read_transaction) => read_transaction,
        Err(err) => {
            tracing::error!(
                "Could not start a read transaction to the database: {}",
                err
            );

            return;
        }
    };

    let program_information_table =
        match read_transaction.open_multimap_table(PROGRAM_INFORMATION_TABLE) {
            Ok(program_information_table) => program_information_table,
            Err(err) => {
                tracing::error!("Could not open program information table: {}", err);

                return;
            }
        };

    if let Ok(program_information_table_iter) = program_information_table.iter().map_err(|err| {
        tracing::error!("Could not iter over program information table: {}", err);
    }) {
        for entry in program_information_table_iter {
            let Ok((program_id_access_guard, program_information_values)) = entry.map_err(|err| {
                tracing::error!(
                    "Could not access entry for program id in hash alias table: {}",
                    err
                );
            }) else {
                continue;
            };
            let program_id = program_id_access_guard.value();

            for program_info_access_guard in program_information_values {
                let Ok(program_info_access_guard) = program_info_access_guard.map_err(|err| {
                    tracing::error!(
                        "Could not access entry in the program information entries for program id \
                         {}: {}",
                        program_id,
                        err
                    );
                }) else {
                    continue;
                };

                let program_info = program_info_access_guard.value();

                for (rom_id, file_name) in
                    program_info
                        .filesystem()
                        .iter()
                        .flat_map(|(rom_id, file_names)| {
                            file_names.iter().map(|file_name| (*rom_id, file_name))
                        })
                {
                    let source_rom_path = rom_store.join(rom_id.to_string());

                    if !source_rom_path.exists() {
                        continue;
                    }

                    let destination_rom_path = match style {
                        ExportStyle::NoIntro => {
                            let machine_folder_name = program_id.machine.to_nointro_string();
                            let machine_folder = destination_path.join(machine_folder_name);
                            let program_folder = machine_folder.join(&program_id.name);
                            let final_path = program_folder.join(file_name);

                            let _ = std::fs::create_dir_all(final_path.parent().unwrap());

                            final_path
                        }
                        ExportStyle::Native => destination_path.join(rom_id.to_string()),
                        ExportStyle::EmulationStation => todo!(),
                    };

                    if !destination_rom_path.starts_with(&destination_path) {
                        tracing::error!("Export path is outside of the target directory");

                        continue;
                    }

                    tracing::info!("Exporting ROM for program {}", program_id);

                    if symlink {
                        if let Err(err) = (|| {
                            #[cfg(target_family = "unix")]
                            return std::os::unix::fs::symlink(
                                source_rom_path,
                                &destination_rom_path,
                            );

                            #[cfg(target_os = "windows")]
                            return std::os::windows::fs::symlink_file(
                                source_rom_path,
                                &destination_rom_path,
                            );

                            #[cfg(not(any(target_family = "unix", target_os = "windows")))]
                            panic!("Unsupported operating system for symlinking");
                        })() {
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
            }
        }
    }
}
