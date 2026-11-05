use std::{
    error::Error,
    fs::File,
    io::{self, BufReader, Cursor, Read, Seek},
    path::{Path, PathBuf},
    pin::Pin,
    sync::{Arc, LazyLock},
};

use bytes::Bytes;
use fluxemu_database::{HASH_ALIAS_TABLE, ProgramDatabase};
use fluxemu_program::{ProgramId, RomId};
use nod::{
    common::Format,
    read::{DiscOptions, DiscReader},
};
use redb::{ReadOnlyMultimapTable, ReadableDatabase};
use rustc_hash::FxBuildHasher;
use sevenz_rust2::Password;
use tokio::{fs, sync::mpsc, task::spawn_blocking};
use zip::ZipArchive;

use crate::{for_each_concurrent, rom::symlink_file};

static ALREADY_FOUND_ROMS: LazyLock<scc::HashSet<RomId, FxBuildHasher>> =
    LazyLock::new(scc::HashSet::default);

type EntryFuture = Pin<Box<dyn Future<Output = Result<(), Box<dyn Error + Send + Sync>>> + Send>>;

trait ReadSeek: Read + Seek {}

impl<T: Read + Seek> ReadSeek for T {}

#[derive(Clone)]
enum ImportSource {
    File(PathBuf),
    Memory(Bytes),
    DecodedDisc(Box<ImportSource>),
}

impl ImportSource {
    fn open(&self) -> Result<Box<dyn ReadSeek>, Box<dyn Error + Send + Sync>> {
        Ok(match self {
            ImportSource::File(path) => Box::new(BufReader::new(File::open(path)?)),
            ImportSource::Memory(bytes) => Box::new(Cursor::new(bytes.clone())),
            ImportSource::DecodedDisc(container) => Box::new(container.open_disc()?),
        })
    }

    fn open_disc(&self) -> nod::Result<DiscReader> {
        let options = DiscOptions::default();

        match self {
            ImportSource::File(path) => DiscReader::new(path, &options),
            ImportSource::Memory(bytes) => {
                DiscReader::new_stream(Box::new(bytes.clone()), &options)
            }
            ImportSource::DecodedDisc(container) => container.open_disc(),
        }
    }
}

struct ImportContext {
    rom_store: PathBuf,
    symlink: bool,
    hash_alias_table: ReadOnlyMultimapTable<RomId, ProgramId>,
}

pub async fn import(
    program_database: &ProgramDatabase,
    rom_store: PathBuf,
    symlink: bool,
    paths: mpsc::UnboundedReceiver<walkdir::DirEntry>,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    let read_transaction = program_database.database().begin_read()?;
    let hash_alias_table = read_transaction.open_multimap_table(HASH_ALIAS_TABLE)?;

    let _ = fs::create_dir_all(&rom_store).await;

    let context = Arc::new(ImportContext {
        rom_store,
        symlink,
        hash_alias_table,
    });

    for_each_concurrent(paths, move |entry| {
        tracing::debug!("Processing file {:?}", entry.path());

        process_entry(context.clone(), ImportSource::File(entry.into_path()))
    })
    .await
}

fn process_entry(context: Arc<ImportContext>, source: ImportSource) -> EntryFuture {
    Box::pin(async move {
        let (sender, receiver) = mpsc::unbounded_channel();

        let scanner = spawn_blocking({
            let context = context.clone();
            let source = source.clone();

            move || scan_entry(&source, &sender, &context.hash_alias_table)
        });

        for_each_concurrent(receiver, |source| process_entry(context.clone(), source)).await?;

        if let Some(rom_id) = scanner.await? {
            store_rom(&context, rom_id, &source).await?;
        }

        Ok(())
    })
}

fn scan_entry(
    source: &ImportSource,
    sender: &mpsc::UnboundedSender<ImportSource>,
    hash_alias_table: &ReadOnlyMultimapTable<RomId, ProgramId>,
) -> Option<RomId> {
    let mut reader = match source.open() {
        Ok(reader) => reader,
        Err(err) => {
            tracing::warn!("Cannot open ROM source: {}", err);
            return None;
        }
    };

    match reader.rewind() {
        Ok(_) => try_as_zip(&mut reader, sender),
        Err(err) => tracing::warn!("Seek failed {}", err),
    }

    match reader.rewind() {
        Ok(_) => try_as_7zip(&mut reader, sender),
        Err(err) => tracing::warn!("Seek failed {}", err),
    }

    match reader.rewind() {
        Ok(_) => try_as_disc(&mut reader, source, sender),
        Err(err) => tracing::warn!("Seek failed {}", err),
    }

    match reader.rewind() {
        Ok(_) => identify_rom(reader, hash_alias_table),
        Err(err) => {
            tracing::warn!("Seek failed {}", err);
            None
        }
    }
}

fn try_as_zip(reader: impl Read + Seek, sender: &mpsc::UnboundedSender<ImportSource>) {
    let mut archive = match ZipArchive::new(reader) {
        Ok(archive) => archive,
        Err(_) => return,
    };

    for index in 0..archive.len() {
        let mut file = match archive.by_index(index) {
            Ok(archive) => archive,
            Err(err) => {
                tracing::warn!("Cannot read ZIP entry {}: {}", index, err);
                continue;
            }
        };

        if !file.is_file() {
            continue;
        }

        if let Ok(file_name) = file.name() {
            tracing::debug!("Processing ZIP entry {}", file_name);
        }

        let mut buffer = Vec::new();
        if let Err(err) = file.read_to_end(&mut buffer) {
            if let Ok(file_name) = file.name() {
                tracing::warn!("Failed to buffer ZIP entry {}: {}", file_name, err);
            }

            continue;
        }

        if sender
            .send(ImportSource::Memory(Bytes::from(buffer)))
            .is_err()
        {
            return;
        }
    }
}

fn try_as_7zip(reader: impl Read + Seek, sender: &mpsc::UnboundedSender<ImportSource>) {
    let mut archive = match sevenz_rust2::ArchiveReader::new(reader, Password::empty()) {
        Ok(archive) => archive,
        Err(_) => return,
    };

    if let Err(err) = archive.for_each_entries(|entry, entry_reader| {
        if entry.is_directory() {
            return Ok(true);
        }

        tracing::debug!("Processing 7z entry {}", entry.name());

        let mut buffer = Vec::new();
        if let Err(err) = entry_reader.read_to_end(&mut buffer) {
            tracing::warn!("Failed to read 7z entry {}: {}", entry.name(), err);
            return Ok(true);
        }

        Ok(sender
            .send(ImportSource::Memory(Bytes::from(buffer)))
            .is_ok())
    }) {
        tracing::warn!("7z scanning error: {}", err);
    }
}

fn try_as_disc(
    mut reader: impl Read,
    source: &ImportSource,
    sender: &mpsc::UnboundedSender<ImportSource>,
) {
    match DiscReader::detect(&mut reader) {
        Ok(Some(Format::Iso) | None) => return,
        Ok(Some(_)) => {}
        Err(err) => {
            tracing::warn!("Cannot detect disc format: {}", err);
            return;
        }
    }

    match source.open_disc() {
        Ok(disc) if disc.meta().lossless => {
            tracing::debug!("Processing {} disc image", disc.meta().format);

            let _ = sender.send(ImportSource::DecodedDisc(Box::new(source.clone())));
        }
        Ok(disc) => {
            tracing::warn!(
                "Cannot losslessly convert {} disc image, only the file itself will be identified",
                disc.meta().format
            );
        }
        Err(err) => tracing::warn!("Cannot open disc image: {}", err),
    }
}

fn identify_rom(
    mut reader: impl Read,
    hash_alias_table: &ReadOnlyMultimapTable<RomId, ProgramId>,
) -> Option<RomId> {
    let rom_id = match RomId::new_sha1(&mut reader) {
        Ok(rom_id) => rom_id,
        Err(err) => {
            tracing::warn!("Failed to hash ROM: {}", err);
            return None;
        }
    };

    if ALREADY_FOUND_ROMS.contains_sync(&rom_id) {
        return None;
    }
    let _ = ALREADY_FOUND_ROMS.insert_sync(rom_id);

    if let Ok(values) = hash_alias_table.get(rom_id) {
        if values.is_empty() {
            return None;
        } else {
            for access_guard in values.into_iter().flatten() {
                let program_id = access_guard.value();

                tracing::info!(
                    "Found ROM {} which is required for program {}",
                    rom_id,
                    program_id
                );
            }
        }
    }

    Some(rom_id)
}

async fn store_rom(
    context: &ImportContext,
    rom_id: RomId,
    source: &ImportSource,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    let rom_store_path = context.rom_store.join(rom_id.to_string());

    if fs::try_exists(&rom_store_path).await.unwrap_or(false) {
        return Ok(());
    }

    let _ = fs::remove_file(&rom_store_path).await;

    match source {
        ImportSource::File(path) => {
            if let Err(err) = store_file(path, &rom_store_path, context.symlink).await {
                tracing::error!("Could not import ROM {}: {}", rom_id, err);
            }
        }
        ImportSource::Memory(bytes) => {
            if let Err(err) = fs::write(&rom_store_path, bytes).await {
                tracing::error!(
                    "Could not output ROM to path {}: {}",
                    rom_store_path.display(),
                    err
                );
            }
        }
        ImportSource::DecodedDisc(_) => {
            let source = source.clone();
            let output_path = rom_store_path.clone();

            if let Err(err) =
                spawn_blocking(move || store_decoded_disc(&source, &output_path)).await?
            {
                tracing::error!(
                    "Could not output ROM to path {}: {}",
                    rom_store_path.display(),
                    err
                );
            }
        }
    }

    Ok(())
}

fn store_decoded_disc(
    source: &ImportSource,
    rom_store_path: &Path,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    let partial_path = rom_store_path.with_extension("partial");

    let result = source.open().and_then(|mut reader| {
        io::copy(&mut reader, &mut File::create(&partial_path)?)?;
        std::fs::rename(&partial_path, rom_store_path)?;

        Ok(())
    });

    if result.is_err() {
        let _ = std::fs::remove_file(&partial_path);
    }

    result
}

async fn store_file(path: &Path, rom_store_path: &Path, symlink: bool) -> io::Result<()> {
    let path = fs::canonicalize(path).await?;

    if symlink {
        symlink_file(&path, rom_store_path).await
    } else {
        fs::copy(path, rom_store_path).await.map(|_| ())
    }
}
