use std::{error::Error, path::Path, sync::Arc};

use fluxemu_database::ProgramDatabase;
use futures::{StreamExt, TryStreamExt, channel::mpsc};
use redb::ReadOnlyDatabase;
use tokio::task::spawn_blocking;

use crate::concurrency_limit;

pub async fn import(
    program_database: Arc<ProgramDatabase>,
    paths: mpsc::UnboundedReceiver<walkdir::DirEntry>,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    paths
        .map(Ok::<_, Box<dyn Error + Send + Sync>>)
        .try_for_each_concurrent(concurrency_limit(), move |entry| {
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
