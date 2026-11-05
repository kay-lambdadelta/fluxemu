use std::{
    collections::BTreeSet,
    error::Error,
    fs::File,
    io::{BufReader, Cursor, Read, Seek, Write},
    path::Path,
    sync::Arc,
};

use fluxemu_database::{HASH_ALIAS_TABLE, PROGRAM_INFORMATION_TABLE, ProgramDatabase};
use fluxemu_environment::Environment;
use fluxemu_program::{ProgramInfo, ProgramManager, RomId, RomStoreSpecification};
use tempfile::tempfile;
use tokio::task::spawn_blocking;

use crate::rom::patch::ups::Ups;

mod ups;

pub trait PatchFormat {
    type Error: Error;
    const EXTENSION: &str;

    fn apply(
        source: impl Read + Seek,
        destination: impl Read + Write + Seek,
        patch: impl Read + Seek,
    ) -> Result<(), Self::Error>;
}

pub async fn patch(
    program_manager: &ProgramManager,
    program_database: Arc<ProgramDatabase>,
    environment: &Environment,
    source: RomId,
    patch: &Path,
    add_to_database: bool,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    let rom = program_manager
        .load(source)
        .await?
        .expect("Could not find rom");

    if let Some(extension) = patch
        .extension()
        .map(|extension| extension.to_string_lossy())
    {
        if extension != Ups::EXTENSION {
            return Err("Not a patch".into());
        }

        let patch_path = patch.to_path_buf();

        let (destination_file, patched_rom_id) = spawn_blocking(
            move || -> Result<(File, RomId), Box<dyn Error + Send + Sync>> {
                let mut destination_file = tempfile()?;
                let patch = BufReader::new(File::open(patch_path)?);

                Ups::apply(&mut Cursor::new(&rom), &mut destination_file, patch)?;

                destination_file.rewind()?;
                let patched_rom_id = RomId::new_sha1(&destination_file)?;
                destination_file.rewind()?;

                Ok((destination_file, patched_rom_id))
            },
        )
        .await??;

        let rom_store_path = environment
            .rom_stores
            .values()
            .find_map(RomStoreSpecification::filesystem_root)
            .ok_or("No ROM store is a local directory")?
            .join(patched_rom_id.to_string());
        tracing::info!(
            "Patch {:?} produced a ROM of ID {} (outputted at path: {:?})",
            patch,
            patched_rom_id,
            rom_store_path
        );

        let mut destination_file = tokio::fs::File::from_std(destination_file);
        let mut rom_store_file = tokio::fs::File::create(&rom_store_path).await?;

        tokio::io::copy(&mut destination_file, &mut rom_store_file).await?;

        let patch_file_name = patch.file_stem().unwrap().to_string_lossy().into_owned();

        if add_to_database {
            spawn_blocking(move || {
                add_patched_rom_to_database(
                    &program_database,
                    source,
                    patched_rom_id,
                    &patch_file_name,
                )
            })
            .await??;
        }
    }

    Ok(())
}

fn add_patched_rom_to_database(
    program_database: &ProgramDatabase,
    source: RomId,
    patched_rom_id: RomId,
    patch_file_name: &str,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    let manifests = program_database.identify_program([source])?;

    if let Some(mut single_rom_manifest) = manifests
        .into_iter()
        .find(|manifest| manifest.info.filesystem().len() == 1)
    {
        single_rom_manifest.id.main_name = patch_file_name.into();
        let mut info = single_rom_manifest.info.mitigate();

        match &mut info {
            ProgramInfo::V0 {
                names,
                filesystem,
                languages,
                version,
            } => {
                // Extract the name and replace it with the patch file name
                if let Some(name) = names.first().cloned()
                    && let Some((_, extension)) = name.rsplit_once('.')
                {
                    let filename = format!("{}.{}", patch_file_name, extension);

                    names.clear();
                    names.insert(filename.clone());

                    filesystem.clear();
                    filesystem.insert(patched_rom_id, BTreeSet::from_iter([filename]));

                    languages.clear();

                    *version = None;

                    // Add to database
                    let database_transaction = program_database.database().begin_write()?;
                    let mut hash_alias_table =
                        database_transaction.open_multimap_table(HASH_ALIAS_TABLE)?;
                    let mut program_information_table =
                        database_transaction.open_multimap_table(PROGRAM_INFORMATION_TABLE)?;

                    hash_alias_table.insert(patched_rom_id, single_rom_manifest.id.clone())?;
                    program_information_table.insert(single_rom_manifest.id.clone(), info)?;

                    drop(hash_alias_table);
                    drop(program_information_table);

                    database_transaction.commit()?;

                    tracing::info!(
                        "Added database entry for patched ROM with ID {}",
                        single_rom_manifest.id
                    );
                } else {
                    tracing::warn!(
                        "Failed to add database entry for patched ROM with ID {}, add manually if \
                         you need this",
                        single_rom_manifest.id
                    );
                }
            }
        }
    }

    Ok(())
}
