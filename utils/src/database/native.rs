use std::error::Error;

use fluxemu_program::{PROGRAM_INFORMATION_TABLE, ProgramManager};
use rayon::iter::{IntoParallelIterator, ParallelIterator};
use redb::{ReadOnlyDatabase, ReadableDatabase, ReadableMultimapTable};

pub fn import(
    program_manager: &ProgramManager,
    paths: impl IntoParallelIterator<Item = Result<walkdir::DirEntry, walkdir::Error>>,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    let internal_database = program_manager.database();

    paths.into_par_iter().try_for_each(|entry| {
        match entry {
            Ok(entry) => {
                let external_database = ReadOnlyDatabase::open(entry.into_path())?;

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
            }
            Err(err) => {
                tracing::error!("Failed to open file: {}", err);
            }
        }

        Ok(())
    })
}
