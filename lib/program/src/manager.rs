use std::{
    collections::{BTreeMap, BTreeSet},
    fmt::Debug,
    path::{Path, PathBuf},
    sync::{Arc, RwLock},
};

use bytes::Bytes;
use opendal::{Operator, services::FS_SCHEME};
use rustc_hash::FxBuildHasher;
use thiserror::Error;
use tokio::{fs::File, io::AsyncReadExt};

use crate::{Manifest, ProgramId, ProgramInfo, RomId, RomStoreSpecification, SystemId};

#[derive(Debug, Error)]
#[non_exhaustive]
pub enum Error {
    #[error("{0}")]
    Io(#[from] std::io::Error),
    #[error("{0}")]
    Join(#[from] tokio::task::JoinError),
    #[error("ROM store {name}: {source}")]
    RomStore {
        name: String,
        source: opendal::Error,
    },
}

struct RomStore {
    name: String,
    operator: Operator,
    filesystem_root: Option<PathBuf>,
}

/// The ROM manager which contains information about the roms that were loaded
pub struct ProgramManager {
    external_roms: scc::HashMap<RomId, PathBuf, FxBuildHasher>,
    embedded_roms: scc::HashMap<RomId, &'static [u8], FxBuildHasher>,
    rom_cache: scc::HashCache<RomId, Bytes>,
    rom_stores: RwLock<Arc<[RomStore]>>,
}

impl ProgramManager {
    pub fn new(
        rom_stores: impl IntoIterator<Item = (String, RomStoreSpecification)>,
    ) -> Result<Arc<Self>, Error> {
        Ok(Arc::new(Self {
            external_roms: scc::HashMap::default(),
            embedded_roms: scc::HashMap::default(),
            rom_cache: scc::HashCache::with_capacity(0, 16),
            rom_stores: RwLock::new(build_rom_stores(rom_stores)?),
        }))
    }

    pub fn set_rom_stores(
        &self,
        rom_stores: impl IntoIterator<Item = (String, RomStoreSpecification)>,
    ) -> Result<(), Error> {
        let rom_stores = build_rom_stores(rom_stores)?;
        *self.rom_stores.write().unwrap() = rom_stores;

        Ok(())
    }

    pub async fn register_external_rom(&self, path: impl AsRef<Path>) -> Result<RomId, Error> {
        let path = path.as_ref();
        let rom_bytes = load_rom_file(path).await?;

        let rom_id = tokio::task::spawn_blocking({
            let rom_bytes = rom_bytes.clone();

            move || RomId::new_sha1(rom_bytes.as_ref())
        })
        .await??;

        self.external_roms
            .upsert_async(rom_id, path.to_path_buf())
            .await;
        let _ = self.rom_cache.put_async(rom_id, rom_bytes).await;

        Ok(rom_id)
    }

    pub async fn load(&self, id: RomId) -> Result<Option<Bytes>, Error> {
        if let Some(rom_bytes) = self.embedded_roms.read_async(&id, |_, bytes| *bytes).await {
            return Ok(Some(Bytes::from_static(rom_bytes)));
        }

        if let Some(rom_bytes) = self
            .rom_cache
            .get_async(&id)
            .await
            .map(|entry| entry.get().clone())
        {
            return Ok(Some(rom_bytes));
        }

        let rom_bytes = if let Some(external_rom_path) = self
            .external_roms
            .read_async(&id, |_, path| path.clone())
            .await
        {
            tracing::info!(
                "Opening ROM {} from external path: {}",
                id,
                external_rom_path.display()
            );

            load_rom_file(&external_rom_path).await?
        } else {
            let Some(rom_bytes) = self.load_from_rom_stores(id).await? else {
                return Ok(None);
            };

            rom_bytes
        };

        let _ = self.rom_cache.put_async(id, rom_bytes.clone()).await;

        Ok(Some(rom_bytes))
    }

    pub async fn auto_generate_specification(
        &self,
        rom_id: RomId,
    ) -> Result<Option<Manifest>, Error> {
        let external_path = self
            .external_roms
            .read_async(&rom_id, |_, path| path.clone())
            .await;
        let rom = self.load(rom_id).await?;

        let Some(machine) = SystemId::guess(external_path.as_deref(), rom.as_deref()) else {
            return Ok(None);
        };

        let (file_name, name) = external_path
            .into_iter()
            .find_map(|path| {
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
            })
            .unwrap_or_else(|| (rom_id.to_string(), rom_id.to_string()));

        let program_id = ProgramId {
            system: machine,
            main_name: name.clone(),
        };

        Ok(Some(Manifest {
            id: program_id,
            info: ProgramInfo::V0 {
                names: BTreeSet::from_iter([name.clone()]),
                filesystem: BTreeMap::from_iter([(rom_id, BTreeSet::from_iter([file_name]))]),
                languages: BTreeSet::default(),
                version: None,
            },
        }))
    }

    pub fn register_embedded_rom(&mut self, bytes: &'static [u8]) -> RomId {
        let rom_id = RomId::new_sha1(bytes).unwrap();

        self.embedded_roms.upsert_sync(rom_id, bytes);

        // No need to cache a rom permanently bolted in
        self.rom_cache.remove_sync(&rom_id);
        self.external_roms.remove_sync(&rom_id);

        rom_id
    }

    async fn load_from_rom_stores(&self, id: RomId) -> Result<Option<Bytes>, Error> {
        let id_as_string = id.to_string();
        let rom_stores = self.rom_stores.read().unwrap().clone();

        for rom_store in rom_stores.iter() {
            if let Some(filesystem_root) = &rom_store.filesystem_root {
                match load_rom_file(&filesystem_root.join(&id_as_string)).await {
                    Ok(rom_bytes) => return Ok(Some(rom_bytes)),
                    Err(Error::Io(err)) if err.kind() == std::io::ErrorKind::NotFound => continue,
                    Err(err) => {
                        tracing::warn!(
                            "Failed to open ROM {} directly from {} (ROM store {}), falling back \
                             to the ROM store operator: {}",
                            id,
                            filesystem_root.display(),
                            rom_store.name,
                            err
                        );
                    }
                }
            }

            match rom_store.operator.read(&id_as_string).await {
                Ok(buffer) => return Ok(Some(buffer.to_bytes())),
                Err(err) if err.kind() == opendal::ErrorKind::NotFound => continue,
                Err(source) => {
                    return Err(Error::RomStore {
                        name: rom_store.name.clone(),
                        source,
                    });
                }
            }
        }

        Ok(None)
    }
}

fn build_rom_stores(
    rom_stores: impl IntoIterator<Item = (String, RomStoreSpecification)>,
) -> Result<Arc<[RomStore]>, Error> {
    rom_stores
        .into_iter()
        .map(|(name, specification)| {
            let operator = match Operator::via_iter(&specification.scheme, specification.options) {
                Ok(operator) => operator,
                Err(source) => return Err(Error::RomStore { name, source }),
            };
            let info = operator.info();
            let filesystem_root = (info.scheme() == FS_SCHEME).then(|| PathBuf::from(info.root()));

            Ok(RomStore {
                name,
                operator,
                filesystem_root,
            })
        })
        .collect()
}

async fn load_rom_file(path: &Path) -> Result<Bytes, Error> {
    let mut rom_file = File::open(path).await?;

    #[cfg(feature = "mmap")]
    {
        use memmap2::Mmap;

        if let Ok(buffer) = unsafe { Mmap::map(&rom_file) } {
            return Ok(Bytes::from_owner(buffer));
        } else {
            tracing::warn!("Failed to memory-map ROM file, falling back to loading into memory");
        }
    }

    let mut buffer = Vec::default();
    rom_file.read_to_end(&mut buffer).await?;

    Ok(Bytes::from_owner(buffer))
}
