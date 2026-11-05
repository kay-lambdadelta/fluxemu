use std::{error::Error, sync::Arc};

use fluxemu_database::ProgramDatabase;
use tokio::{fs, sync::mpsc, task::spawn_blocking};

use crate::{for_each_concurrent, logiqx::add_to_database};

pub async fn import(
    program_database: Arc<ProgramDatabase>,
    paths: mpsc::UnboundedReceiver<walkdir::DirEntry>,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    for_each_concurrent(paths, move |entry| {
        let program_database = program_database.clone();

        async move {
            let bytes = fs::read(entry.into_path()).await?;

            spawn_blocking(move || add_to_database(&program_database, bytes.as_slice())).await?
        }
    })
    .await
}
