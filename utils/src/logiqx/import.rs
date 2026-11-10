use std::{
    error::Error,
    sync::{Arc, Mutex},
};

use fluxemu_database::ProgramDatabase;
use fluxemu_program::SystemId;
use tokio::{fs, sync::mpsc, task::spawn_blocking};

use crate::{
    for_each_concurrent,
    logiqx::{add_to_database, parse},
};

pub async fn import(
    program_database: Arc<ProgramDatabase>,
    paths: mpsc::UnboundedReceiver<walkdir::DirEntry>,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    let data_files = Arc::new(Mutex::new(Vec::new()));

    for_each_concurrent(paths, {
        let data_files = data_files.clone();

        move |entry| {
            let data_files = data_files.clone();

            async move {
                let path = entry.into_path();
                let bytes = fs::read(&path).await?;

                match spawn_blocking(move || parse(bytes.as_slice())).await? {
                    Ok(data_file) => match SystemId::from_nointro_str(&data_file.header.name) {
                        Ok(system) => data_files.lock().unwrap().push((system, data_file)),
                        Err(err) => {
                            tracing::error!("Skipping logiqx database {}: {}", path.display(), err);
                        }
                    },
                    Err(err) => {
                        tracing::error!(
                            "Failed to parse logiqx database {}: {}",
                            path.display(),
                            err
                        );
                    }
                }

                Ok(())
            }
        }
    })
    .await?;

    let data_files = std::mem::take(&mut *data_files.lock().unwrap());

    spawn_blocking(move || add_to_database(&program_database, data_files)).await?
}
