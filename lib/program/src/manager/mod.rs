use std::{
    collections::{BTreeMap, BTreeSet},
    fmt::Debug,
    fs::File,
    io::Read,
    ops::Deref,
    path::{Path, PathBuf},
    sync::Arc,
};

use bytes::Bytes;
use redb::{Database, MultimapTableDefinition, ReadableDatabase, backends::InMemoryBackend};
use rustc_hash::FxBuildHasher;
use sha1::{Digest, Sha1};
use thiserror::Error;

use crate::{MachineId, ProgramId, ProgramInfo, ProgramSpecification, RomId};

#[derive(Debug, Error)]
pub enum Error {
    #[error("{0}")]
    RedbTransaction(#[from] redb::TransactionError),
    #[error("{0}")]
    RedbTable(#[from] redb::TableError),
    #[error("{0}")]
    RedbStorage(#[from] redb::StorageError),
    #[error("{0}")]
    RedbCommit(#[from] redb::CommitError),
    #[error("{0}")]
    Redb(#[from] redb::Error),
    #[error("{0}")]
    Io(#[from] std::io::Error),
    #[error("{0}")]
    Ron(#[from] ron::Error),
    #[error("{0}")]
    RonSpanned(#[from] ron::error::SpannedError),
}

/// Program id -> Program info mapping
pub const PROGRAM_INFORMATION_TABLE: MultimapTableDefinition<ProgramId, ProgramInfo> =
    MultimapTableDefinition::new("program_information");
/// Hash -> Program id reverse mapping
pub const HASH_ALIAS_TABLE: MultimapTableDefinition<RomId, ProgramId> =
    MultimapTableDefinition::new("hash_alias");

/// The ROM manager which contains the database and information about the roms that were loaded
#[derive(Debug)]
pub struct ProgramManager {
    database: Arc<Database>,
    external_roms: scc::HashMap<RomId, PathBuf, FxBuildHasher>,
    rom_cache: scc::HashCache<RomId, Bytes>,
    rom_stores: Vec<PathBuf>,
}

impl ProgramManager {
    /// Opens and loads the default database
    pub fn new(database: Database) -> Result<Arc<Self>, Error> {
        let mut database_transaction = database.begin_write()?;
        database_transaction.set_quick_repair(true);
        database_transaction.open_multimap_table(PROGRAM_INFORMATION_TABLE)?;
        database_transaction.open_multimap_table(HASH_ALIAS_TABLE)?;
        database_transaction.commit()?;

        let database = Arc::new(database);

        Ok(Arc::new(Self {
            database,
            external_roms: scc::HashMap::default(),
            rom_cache: scc::HashCache::with_capacity(0, 16),
            rom_stores: Vec::default(),
        }))
    }

    pub fn register_external(&self, path: impl AsRef<Path>) -> Result<RomId, Error> {
        let path = path.as_ref();

        #[allow(unused_mut)]
        let mut rom_file = File::open(path)?;

        // Memmap the file on supported platforms
        #[cfg(any(target_family = "unix", target_os = "windows"))]
        let rom_bytes = {
            use memmap2::Mmap;
            let rom_bytes = unsafe { Mmap::map(&rom_file) }?;

            Bytes::from_owner(rom_bytes)
        };

        #[cfg(not(any(target_family = "unix", target_os = "windows")))]
        let rom_bytes = {
            let mut rom_bytes = Vec::new();
            rom_file.read_to_end(&mut rom_bytes).await?;

            Bytes::from(rom_bytes)
        };

        // Find the ID of the rom
        let mut hasher = Sha1::new();
        hasher.update(&rom_bytes);
        let hash = hasher.finalize();
        let rom_id = RomId(hash.into());

        self.external_roms.upsert_sync(rom_id, path.to_path_buf());
        let _ = self.rom_cache.put_sync(rom_id, rom_bytes);

        Ok(rom_id)
    }

    pub fn load(&self, id: RomId) -> Result<Option<Bytes>, Error> {
        match self.rom_cache.entry_sync(id) {
            scc::hash_cache::Entry::Occupied(bytes) => Ok(Some(bytes.clone())),
            scc::hash_cache::Entry::Vacant(vacant_entry) => {
                if let Some(external_rom_path) = self.external_roms.get_sync(&id) {
                    tracing::info!(
                        "Opening ROM {} from external path: {}",
                        id,
                        external_rom_path.display()
                    );

                    let rom_file = File::open(external_rom_path.deref())?;

                    return load_rom_bytes(rom_file).map(|rom| {
                        vacant_entry.put_entry(rom.clone());

                        Some(rom)
                    });
                }

                let id_as_string = id.to_string();

                for rom_store in &self.rom_stores {
                    let rom_path = rom_store.join(&id_as_string);
                    let Ok(rom_file) = File::open(rom_path) else {
                        continue;
                    };

                    return load_rom_bytes(rom_file).map(|rom| {
                        vacant_entry.put_entry(rom.clone());
                        Some(rom)
                    });
                }

                Ok(None)
            }
        }
    }

    /// For testing purposes
    pub fn dummy() -> Result<Arc<Self>, Error> {
        let database = Database::builder()
            .create_with_backend(InMemoryBackend::default())
            .unwrap();

        Self::new(database)
    }

    /// Attempts to identify a program from its program ids
    #[tracing::instrument(skip_all)]
    pub fn identify_program(&self, roms: &[RomId]) -> Result<Vec<ProgramSpecification>, Error> {
        let read_transaction = self.database().begin_read()?;
        let hash_alias_table = read_transaction.open_multimap_table(HASH_ALIAS_TABLE)?;
        let program_info_table = read_transaction.open_multimap_table(PROGRAM_INFORMATION_TABLE)?;

        let mut possible_programs = Vec::default();

        for rom_id in roms {
            for access_guard in hash_alias_table.get(rom_id)? {
                let program_id = access_guard?.value();

                for access_guard in program_info_table.get(&program_id)? {
                    let program_info = access_guard?.value();

                    let found_all = roms
                        .iter()
                        .all(|id| program_info.filesystem().contains_key(id));

                    if found_all {
                        possible_programs.push(ProgramSpecification {
                            id: program_id.clone(),
                            info: program_info,
                        });
                    }
                }
            }
        }

        Ok(possible_programs)
    }

    pub fn auto_generate_specification(
        &self,
        rom_id: RomId,
    ) -> Result<Option<ProgramSpecification>, Error> {
        let external_path = self.external_roms.get_sync(&rom_id);
        let rom = self.load(rom_id)?;

        let Some(machine) = MachineId::guess(
            external_path.as_deref().map(|path| path.as_path()),
            rom.as_deref(),
        ) else {
            return Ok(None);
        };

        let Some((file_name, name)) = external_path.into_iter().find_map(|path| {
            let file_name = path.file_name().unwrap().to_string_lossy().to_string();
            let name = file_name
                .split('.')
                .next()
                .unwrap_or(&file_name)
                .to_string();

            if name.is_empty() {
                return None;
            }

            Some((file_name, name))
        }) else {
            return Ok(None);
        };

        let program_id = ProgramId {
            machine,
            name: name.clone(),
        };

        Ok(Some(ProgramSpecification {
            id: program_id,
            info: ProgramInfo::V0 {
                names: BTreeSet::from_iter([name.clone()]),
                filesystem: BTreeMap::from_iter([(rom_id, BTreeSet::from_iter([file_name]))]),
                languages: BTreeSet::default(),
                version: None,
            },
        }))
    }

    pub fn database(&self) -> &Database {
        &self.database
    }
}

fn load_rom_bytes(mut rom_file: File) -> Result<Bytes, Error> {
    #[cfg(any(target_os = "windows", target_family = "unix"))]
    {
        use memmap2::Mmap;

        if let Ok(buffer) = unsafe { Mmap::map(&rom_file) } {
            return Ok(Bytes::from_owner(buffer));
        }
    }

    let mut buffer = Vec::default();
    rom_file.read_to_end(&mut buffer)?;

    Ok(Bytes::from_owner(buffer))
}
