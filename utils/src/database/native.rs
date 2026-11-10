use std::{error::Error, path::Path, sync::Arc};

use fluxemu_database::ProgramDatabase;
use redb::ReadOnlyDatabase;
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
    let external_database = ProgramDatabase::read_only(ReadOnlyDatabase::open(path)?)?;

    let writer = program_database.write()?;
    writer.merge_from(&external_database)?;
    writer.commit()?;

    Ok(())
}
