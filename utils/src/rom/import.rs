use std::{
    fs::{File, create_dir_all},
    io::{Cursor, Read, Seek, SeekFrom},
    path::{Path, PathBuf},
    sync::{Arc, LazyLock},
};

use fluxemu_program::{HASH_ALIAS_TABLE, ProgramId, ProgramManager, RomId};
use rayon::{
    Scope,
    iter::{IntoParallelIterator, ParallelBridge, ParallelIterator},
};
use redb::{ReadOnlyMultimapTable, ReadableDatabase};
use rustc_hash::FxBuildHasher;
use sevenz_rust2::Password;
use walkdir::WalkDir;
use zip::ZipArchive;

static ALREADY_FOUND_ROMS: LazyLock<scc::HashSet<RomId, FxBuildHasher>> =
    LazyLock::new(scc::HashSet::default);

pub fn rom_import(
    paths: impl IntoParallelIterator<Item = PathBuf> + Send,
    program_manager: Arc<ProgramManager>,
    rom_store: &Path,
    symlink: bool,
) {
    let read_transaction = match program_manager.database().begin_read() {
        Ok(read_transaction) => read_transaction,
        Err(err) => {
            tracing::error!(
                "Could not start a read transaction to the database: {}",
                err
            );

            return;
        }
    };

    let hash_alias_table = match read_transaction.open_multimap_table(HASH_ALIAS_TABLE) {
        Ok(hash_alias_table) => hash_alias_table,
        Err(err) => {
            tracing::error!("Could not open hash alias table: {}", err);

            return;
        }
    };

    let _ = create_dir_all(rom_store);

    rayon::scope(|scope| {
        paths
            .into_par_iter()
            .for_each(|path| match path.metadata() {
                Err(err) => {
                    tracing::warn!("Cannot stat {}: {}", path.display(), err);
                }
                Ok(metadata) if metadata.is_file() => match File::open(&path) {
                    Ok(file) => {
                        tracing::debug!("Processing file {}", path.display());

                        process_entry(
                            scope,
                            file,
                            Some(&path),
                            rom_store,
                            symlink,
                            &hash_alias_table,
                        );
                    }
                    Err(err) => tracing::warn!("Cannot open {}: {}", path.display(), err),
                },
                Ok(metadata) if metadata.is_dir() => {
                    WalkDir::new(path)
                        .follow_links(true)
                        .into_iter()
                        .filter_map(|result| match result {
                            Ok(entry) if entry.file_type().is_file() => Some(entry),
                            Ok(_) => None,
                            Err(err) => {
                                tracing::warn!("Directory walk error: {}", err);
                                None
                            }
                        })
                        .par_bridge()
                        .for_each(|entry| match File::open(entry.path()) {
                            Ok(file) => {
                                tracing::debug!("Processing file {}", entry.path().display());

                                process_entry(
                                    scope,
                                    file,
                                    Some(entry.path()),
                                    rom_store,
                                    symlink,
                                    &hash_alias_table,
                                );
                            }
                            Err(err) => {
                                tracing::warn!("Cannot open {}: {}", entry.path().display(), err)
                            }
                        });
                }
                Ok(_) => {
                    tracing::warn!("Skipping {}: not a file or directory", path.display());
                }
            });
    });
}

fn process_entry<'a>(
    scope: &Scope<'a>,
    mut reader: impl Read + Seek,
    path: Option<&Path>,
    rom_store: impl AsRef<Path>,
    symlink: bool,
    hash_alias_table: &'a ReadOnlyMultimapTable<RomId, ProgramId>,
) {
    if let Some(path) = path
        && let Some(extension) = path.extension()
        && let Some(extension) = extension.to_str()
        && ["gcz", "wia", "rvz"].contains(&extension)
    {
        tracing::warn!("This ROM probably needs to be converted to be recognized");
    }

    let rom_store = rom_store.as_ref();

    try_as_zip(scope, &mut reader, rom_store, hash_alias_table);

    match reader.seek(SeekFrom::Start(0)) {
        Ok(_) => try_as_7zip(scope, &mut reader, rom_store, hash_alias_table),
        Err(err) => tracing::warn!("Seek failed {}", err),
    }

    match reader.seek(SeekFrom::Start(0)) {
        Ok(_) => process_rom(reader, path, rom_store, symlink, hash_alias_table),
        Err(err) => tracing::warn!("Seek failed {}", err),
    }
}

fn try_as_zip<'a>(
    scope: &Scope<'a>,
    reader: impl Read + Seek,
    rom_store: &Path,
    hash_alias_table: &'a ReadOnlyMultimapTable<RomId, ProgramId>,
) {
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

        tracing::debug!("Processing ZIP entry {}", file.name());

        let mut buffer = Vec::new();
        if let Err(err) = file.read_to_end(&mut buffer) {
            tracing::warn!("Failed to buffer ZIP entry {}: {}", file.name(), err);

            continue;
        }

        let rom_store = rom_store.to_path_buf();

        scope.spawn(|scope| {
            process_entry(
                scope,
                Cursor::new(buffer),
                None,
                rom_store,
                false,
                hash_alias_table,
            );
        });
    }
}

fn try_as_7zip<'a>(
    scope: &Scope<'a>,
    reader: impl Read + Seek,
    rom_store: &Path,
    hash_alias_table: &'a ReadOnlyMultimapTable<RomId, ProgramId>,
) {
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

        let rom_store = rom_store.to_path_buf();

        scope.spawn(|scope| {
            process_entry(
                scope,
                Cursor::new(buffer),
                None,
                rom_store,
                false,
                hash_alias_table,
            );
        });

        Ok(true)
    }) {
        tracing::warn!("7z scanning error: {}", err);
    }
}

fn process_rom(
    mut reader: impl Read + Seek,
    path: Option<&Path>,
    rom_store: &Path,
    symlink: bool,
    hash_alias_table: &ReadOnlyMultimapTable<RomId, ProgramId>,
) {
    let rom_id = RomId::new_sha1(&mut reader).unwrap();

    if ALREADY_FOUND_ROMS.contains_sync(&rom_id) {
        return;
    }
    let _ = ALREADY_FOUND_ROMS.insert_sync(rom_id);

    if let Ok(values) = hash_alias_table.get(rom_id) {
        if values.is_empty() {
            return;
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
    reader.rewind().unwrap();

    let rom_store_path = rom_store.join(rom_id.to_string());

    if !rom_store_path.exists() {
        let _ = std::fs::remove_file(&rom_store_path);

        if let Some(path) = path
            && let Ok(path) = path.canonicalize()
        {
            if symlink {
                if let Err(err) = (|| {
                    #[cfg(target_family = "unix")]
                    return std::os::unix::fs::symlink(path, &rom_store_path);

                    #[cfg(target_os = "windows")]
                    return std::os::windows::fs::symlink_file(path, &rom_store_path);

                    #[cfg(not(any(target_family = "unix", target_os = "windows")))]
                    panic!("Unsupported operating system for symlinking");
                })() {
                    tracing::error!("Could not import ROM {}: {}", rom_id, err);
                }
            } else {
                if let Err(err) = std::fs::copy(path, &rom_store_path) {
                    tracing::error!("Could not import ROM {}: {}", rom_id, err);
                }
            }
        } else {
            if let Err(err) = File::create(&rom_store_path)
                .and_then(|mut file| std::io::copy(&mut reader, &mut file))
            {
                tracing::error!(
                    "Could not output ROM to path {}: {}",
                    rom_store_path.display(),
                    err
                );
            }
        }
    }
}
