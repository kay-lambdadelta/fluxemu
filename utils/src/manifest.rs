use std::{error::Error, path::PathBuf, sync::Arc};

use fluxemu_database::{PROGRAM_INFORMATION_TABLE, ProgramDatabase};
use fluxemu_program::Manifest;
use redb::{ReadableDatabase, ReadableMultimapTable};
use ron::ser::PrettyConfig;
use tokio::{fs, sync::mpsc, task::spawn_blocking};

use crate::for_each_concurrent;

pub async fn import(
    program_database: Arc<ProgramDatabase>,
    paths: mpsc::UnboundedReceiver<walkdir::DirEntry>,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    for_each_concurrent(paths, move |entry| {
        let program_database = program_database.clone();

        async move {
            let bytes = fs::read(entry.path()).await?;

            spawn_blocking(move || -> Result<(), Box<dyn Error + Send + Sync>> {
                let database_transaction = program_database.database().begin_write()?;
                let mut database_table =
                    database_transaction.open_multimap_table(PROGRAM_INFORMATION_TABLE)?;

                match ron::Options::default().from_bytes::<Manifest>(&bytes) {
                    Ok(program_specification) => {
                        database_table
                            .insert(program_specification.id, program_specification.info)?;
                    }
                    Err(err) => {
                        tracing::warn!("Failed to open file {:?}: {}", entry.path(), err);
                    }
                }

                Ok(())
            })
            .await?
        }
    })
    .await
}

pub async fn export(
    program_database: Arc<ProgramDatabase>,
    output_directory: PathBuf,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    let _ = fs::create_dir_all(&output_directory).await;

    let (sender, receiver) = mpsc::unbounded_channel();

    let table_reader = spawn_blocking(move || -> Result<(), Box<dyn Error + Send + Sync>> {
        let database_transaction = program_database.database().begin_read()?;
        let database_table = database_transaction.open_multimap_table(PROGRAM_INFORMATION_TABLE)?;

        for item in database_table.iter()? {
            let (program_id, program_infos) = item?;

            for program_info in program_infos {
                let specification = Manifest {
                    id: program_id.value(),
                    info: program_info?.value(),
                };

                if sender.send(specification).is_err() {
                    return Ok(());
                }
            }
        }

        Ok(())
    });

    for_each_concurrent(receiver, move |specification| {
        let mut output_path = output_directory.join(specification.id.to_string());
        output_path.set_extension("ron");

        async move {
            let output = ron::Options::default()
                .to_string_pretty(&specification, PrettyConfig::default())?;

            fs::write(output_path, output).await?;

            Ok(())
        }
    })
    .await?;

    table_reader.await?
}
