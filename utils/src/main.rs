use std::{error::Error, fmt::Display, fs::File, io::BufReader, path::PathBuf};

use clap::{Parser, ValueEnum};
use fluxemu_environment::find_and_load_environment;
use fluxemu_program::{MachineId, PROGRAM_INFORMATION_TABLE, ProgramManager};
use rayon::iter::{IntoParallelIterator, ParallelIterator};
use redb::{Database, ReadOnlyDatabase, ReadableDatabase, ReadableMultimapTable};

use crate::{
    redump::{RedumpSystem, download_and_import_redump_system},
    rom::{export::rom_export, import::rom_import},
};

mod logiqx;
mod redump;
mod rom;

#[derive(Clone, Debug, Default, ValueEnum)]
pub enum ExportStyle {
    #[default]
    NoIntro,
    Native,
    EmulationStation,
}

impl Display for ExportStyle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}",
            match self {
                ExportStyle::NoIntro => "no-intro",
                ExportStyle::Native => "native",
                ExportStyle::EmulationStation => "emulationstation",
            }
        )
    }
}

#[derive(Clone, Parser)]
pub enum Cli {
    /// Import logiqx format datasheet
    ImportLogiqxDatasheet {
        #[clap(required=true, num_args=1..)]
        paths: Vec<PathBuf>,
    },
    /// Import native [redb] format databases into the internal one
    ImportDatabase {
        #[clap(required=true, num_args=1..)]
        paths: Vec<PathBuf>,
    },
    /// Download Redump datasheets (logiqx format)
    DownloadRedumpDatasheet {
        #[clap(long, num_args=1..)]
        system_filter: Vec<MachineId>,
    },
    /// Import roms into a rom store
    ImportRoms {
        /// Symlink instead of copying, where supported
        #[clap(short = 'l', long)]
        symlink: bool,
        /// Paths to search
        #[clap(required=true, num_args=1..)]
        paths: Vec<PathBuf>,
    },
    /// Export ROMs for more friendly access
    ExportRoms {
        /// Symlink instead of copying, where supported
        #[clap(short = 'l', long)]
        symlink: bool,
        /// Set the style of the destination directory
        #[clap(long, default_value_t=ExportStyle::default())]
        style: ExportStyle,
        /// Destination directory
        destination: PathBuf,
    },
    /// Verify ROMs within stores
    VerifyRoms,
}

fn main() -> Result<(), Box<dyn Error + Send + Sync>> {
    let (_, environment) = find_and_load_environment();

    tracing_subscriber::fmt().init();

    let args = Cli::parse();

    let program_manager = ProgramManager::new(
        Database::create(&environment.database_location)?,
        environment.rom_store_directories.clone(),
    )?;

    match args {
        Cli::ImportLogiqxDatasheet { mut paths } => {
            paths.dedup();

            paths.into_par_iter().try_for_each(|path| {
                let file = File::open(path)?;

                logiqx::import(BufReader::new(file), &program_manager)?;

                Ok::<_, Box<dyn Error + Send + Sync>>(())
            })?;
        }
        Cli::ImportDatabase { mut paths } => {
            paths.dedup();

            let internal_database = program_manager.database();

            paths.into_par_iter().try_for_each(|path| {
                let external_database = ReadOnlyDatabase::open(path.clone())?;

                let external_database_transaction = external_database.begin_read()?;
                let external_database_table =
                    external_database_transaction.open_multimap_table(PROGRAM_INFORMATION_TABLE)?;

                let internal_database_transaction = internal_database.begin_write()?;
                let mut internal_database_table =
                    internal_database_transaction.open_multimap_table(PROGRAM_INFORMATION_TABLE)?;

                for item in external_database_table.iter()? {
                    let (rom_id, rom_infos) = item?;

                    for rom_info in rom_infos {
                        let rom_info = rom_info?;

                        internal_database_table.insert(rom_id.value(), rom_info.value())?;
                    }
                }

                drop(internal_database_table);
                internal_database_transaction.commit()?;

                Ok::<_, Box<dyn Error + Send + Sync>>(())
            })?;
        }
        Cli::DownloadRedumpDatasheet { system_filter } => {
            if system_filter.is_empty() {
                for machine_id in MachineId::iter() {
                    if !system_filter.contains(&machine_id)
                        && let Ok(redump_system) = RedumpSystem::try_from(machine_id)
                    {
                        download_and_import_redump_system(redump_system, program_manager.clone())?;
                    }
                }
            } else {
                for machine_id in system_filter {
                    if let Ok(redump_system) = RedumpSystem::try_from(machine_id) {
                        download_and_import_redump_system(redump_system, program_manager.clone())?;
                    }
                }
            }
        }
        Cli::ImportRoms { paths, symlink } => {
            rom_import(
                paths,
                program_manager,
                &environment.rom_store_directories[0],
                symlink,
            );
        }
        Cli::ExportRoms {
            symlink,
            style,
            destination,
        } => {
            rom_export(
                destination,
                program_manager,
                &environment.rom_store_directories,
                symlink,
                style,
            );
        }
        Cli::VerifyRoms => {}
    }

    Ok(())
}
