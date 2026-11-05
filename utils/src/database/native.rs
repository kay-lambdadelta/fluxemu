use std::{error::Error, path::Path, sync::Arc};

use fluxemu_database::{PROGRAM_INFORMATION_TABLE, ProgramDatabase};
use redb::{ReadOnlyDatabase, ReadableDatabase, ReadableMultimapTable};
use tokio::{sync::mpsc, task::spawn_blocking};

use crate::for_each_concurrent;

pub async fn import(
    program_database: Arc<ProgramDatabase>,
    paths: mpsc::UnboundedReceiver<walkdir::DirEntry>,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    for_each_concurrent(paths, move |entry| {
        let program_database = program_database.clone();

        async move {
            spawn_blocking(move || import_database(&program_database, entry.path())).await?
        }
    })
    .await
}

fn import_database(
    program_database: &ProgramDatabase,
    path: &Path,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    let internal_database = program_database.database();

    let external_database = ReadOnlyDatabase::open(path)?;

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

    Ok(())
}
