use std::{error::Error, path::PathBuf, sync::Arc};

use fluxemu_database::ProgramDatabase;
use fluxemu_program::{ProgramId, ProgramInfo, Variant};
use ron::ser::PrettyConfig;
use serde::{Deserialize, Serialize};
use tokio::{fs, sync::mpsc, task::spawn_blocking};

use crate::for_each_concurrent;

#[derive(Debug, Serialize, Deserialize)]
struct ManifestFile {
    id: ProgramId,
    info: ProgramInfo,
    variants: Vec<Variant>,
}

pub async fn import(
    program_database: Arc<ProgramDatabase>,
    paths: mpsc::UnboundedReceiver<walkdir::DirEntry>,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    for_each_concurrent(paths, move |entry| {
        let program_database = program_database.clone();

        async move {
            let bytes = fs::read(entry.path()).await?;

            spawn_blocking(move || -> Result<(), Box<dyn Error + Send + Sync>> {
                let manifest = match ron::Options::default().from_bytes::<ManifestFile>(&bytes) {
                    Ok(manifest) => manifest,
                    Err(err) => {
                        tracing::warn!("Failed to open file {:?}: {}", entry.path(), err);

                        return Ok(());
                    }
                };

                let writer = program_database.write()?;
                writer.insert_program(&manifest.id, &manifest.info)?;

                for variant in &manifest.variants {
                    writer.insert_variant(&manifest.id, variant)?;
                }

                writer.commit()?;

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
        for (id, info) in program_database.programs(None)? {
            let variants = program_database.variants(&id)?;

            if sender.send(ManifestFile { id, info, variants }).is_err() {
                return Ok(());
            }
        }

        Ok(())
    });

    for_each_concurrent(receiver, move |manifest: ManifestFile| {
        let mut output_path = output_directory.join(manifest.id.path());
        output_path.as_mut_os_string().push(".ron");

        async move {
            if let Some(parent) = output_path.parent() {
                fs::create_dir_all(parent).await?;
            }

            let output =
                ron::Options::default().to_string_pretty(&manifest, PrettyConfig::default())?;

            fs::write(output_path, output).await?;

            Ok(())
        }
    })
    .await?;

    table_reader.await?
}
