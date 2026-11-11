use std::{
    collections::{BTreeMap, BTreeSet},
    error::Error,
    fs::File,
    io::{BufReader, Cursor, Read, Seek, Write},
    path::Path,
    sync::Arc,
};

use bytes::Bytes;
use fluxemu_database::ProgramDatabase;
use fluxemu_environment::Environment;
use fluxemu_program::{
    Derivative, DerivativeKind, DumpStatus, FileDigest, Manifest, NonCanonicalHeader, ProgramFlag,
    ProgramId, ProgramInfo, ProgramManager, RomFile, RomId, RomStoreSpecification, Title, Variant,
};
use tempfile::tempfile;
use tokio::task::spawn_blocking;

use crate::{cli::LanguageChange, rom::patch::ups::Ups};

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
    translation: &[LanguageChange],
) -> Result<(), Box<dyn Error + Send + Sync>> {
    let Some(extension) = patch
        .extension()
        .map(|extension| extension.to_string_lossy())
    else {
        return Ok(());
    };

    if extension != Ups::EXTENSION {
        return Err("Not a patch".into());
    }

    let digest = program_manager
        .digest(source)
        .await?
        .ok_or("Could not find ROM")?;

    let source_manifest = spawn_blocking({
        let program_database = program_database.clone();

        move || program_database.identify_program(&[digest])
    })
    .await??
    .into_iter()
    .map(|program_match| program_match.manifest)
    .find(|manifest| manifest.variant.files().len() == 1);

    let (rom, header_length) = match &source_manifest {
        Some(manifest) => {
            let (path, file) = manifest.variant.files().first_key_value().unwrap();
            let file_match = &manifest.files[path];

            let rom = program_manager
                .load_file(file, file_match)
                .await?
                .ok_or("Could not find ROM")?;
            let header_length = file.header_length();

            (rom, header_length)
        }
        None => (
            program_manager
                .load(source)
                .await?
                .ok_or("Could not find ROM")?,
            0,
        ),
    };

    let patch_path = patch.to_path_buf();

    let patched = spawn_blocking(move || -> Result<Vec<u8>, Box<dyn Error + Send + Sync>> {
        let mut destination_file = tempfile()?;
        let patch = BufReader::new(File::open(patch_path)?);

        Ups::apply(&mut Cursor::new(&rom), &mut destination_file, patch)?;

        destination_file.rewind()?;
        let mut patched = Vec::new();
        destination_file.read_to_end(&mut patched)?;

        Ok(patched)
    })
    .await??;

    let header_length = if patched.len() >= header_length {
        header_length
    } else {
        0
    };
    let (header, contents) = patched.split_at(header_length);

    let contents_digest = FileDigest::new(contents)?;
    let patched_rom_id = contents_digest.id;

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

    tokio::fs::write(&rom_store_path, contents).await?;

    if add_to_database && let Some(source_manifest) = source_manifest {
        let patch_name = patch.file_stem().unwrap().to_string_lossy().into_owned();
        let non_canonical_header = (!header.is_empty())
            .then(|| -> Result<_, Box<dyn Error + Send + Sync>> {
                Ok(NonCanonicalHeader {
                    bytes: Bytes::copy_from_slice(header),
                    hashes_with_header: FileDigest::new(patched.as_slice())?.hashes,
                })
            })
            .transpose()?;

        let translation = translation.to_vec();

        spawn_blocking(move || {
            add_patched_rom_to_database(
                &program_database,
                source_manifest,
                contents_digest,
                non_canonical_header,
                patch_name,
                &translation,
            )
        })
        .await??;
    }

    Ok(())
}

fn add_patched_rom_to_database(
    program_database: &ProgramDatabase,
    source_manifest: Manifest,
    contents_digest: FileDigest,
    non_canonical_header: Option<NonCanonicalHeader>,
    patch_name: String,
    translation: &[LanguageChange],
) -> Result<(), Box<dyn Error + Send + Sync>> {
    let (path, _) = source_manifest.variant.files().first_key_value().unwrap();
    let extension = path.rsplit_once('.').map(|(_, extension)| extension);

    let patched_path = match extension {
        Some(extension) => format!("{patch_name}.{extension}"),
        None => patch_name.clone(),
    };

    let program_id = ProgramId {
        system: source_manifest.id.system,
        main_name: patch_name.clone(),
    };

    let ProgramInfo::V0 {
        regions,
        mut languages,
        video_standards,
        ..
    } = source_manifest.info;

    for change in translation {
        match *change {
            LanguageChange::Add(language) => {
                languages.insert(language);
            }
            LanguageChange::Remove(language) => {
                languages.remove(&language);
            }
        }
    }

    let flag = if translation.is_empty() {
        ProgramFlag::Hack
    } else {
        ProgramFlag::Translation
    };

    let info = ProgramInfo::V0 {
        title: Title {
            text: patch_name,
            language: None,
        },
        alternative_titles: BTreeSet::new(),
        regions,
        languages,
        revision: None,
        kind: Default::default(),
        flags: BTreeSet::from([flag]),
        video_standards,
        serials: BTreeSet::new(),
    };

    let variant = Variant::V0 {
        files: BTreeMap::from([(
            patched_path,
            RomFile {
                size: contents_digest.size,
                rom_id: Some(contents_digest.id),
                hashes: contents_digest.hashes,
                status: DumpStatus::Unverified,
                non_canonical_header,
            },
        )]),
    };

    let writer = program_database.write()?;
    writer.insert_program(&program_id, &info)?;
    writer.insert_variant(&program_id, &variant)?;
    writer.insert_derivative(
        &source_manifest.id,
        &Derivative {
            program: program_id.clone(),
            kind: DerivativeKind::Patch,
        },
    )?;
    writer.commit()?;

    tracing::info!(
        "Added database entry for patched ROM with ID {}",
        program_id
    );

    Ok(())
}
