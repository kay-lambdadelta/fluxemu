use std::{
    collections::{BTreeMap, BTreeSet},
    fmt::Debug,
    path::{Path, PathBuf},
    sync::{Arc, RwLock},
};

use bytes::{BufMut, Bytes, BytesMut};
use opendal::{Operator, services::FS_SCHEME};
use rustc_hash::FxBuildHasher;
use thiserror::Error;
use tokio::{fs::File, io::AsyncReadExt};

use crate::{
    DumpStatus, FileDigest, FileMatch, Manifest, MatchStrength, ProgramId, ProgramInfo, RomFile,
    RomForm, RomHashes, RomId, RomStoreSpecification, SystemId, Title, Variant,
};

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
    digests: scc::HashMap<RomId, FileDigest, FxBuildHasher>,
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
            digests: scc::HashMap::default(),
            rom_stores: RwLock::new(build_rom_stores(rom_stores)?),
        }))
    }

    pub fn remember_digest(&self, digest: &FileDigest) {
        self.digests.upsert_sync(digest.id, digest.clone());
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

        let digest = tokio::task::spawn_blocking({
            let rom_bytes = rom_bytes.clone();

            move || FileDigest::new(rom_bytes.as_ref())
        })
        .await??;
        let rom_id = digest.id;

        self.digests.upsert_async(rom_id, digest).await;

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

    /// If this was a ROM with a non canonical header that was stripped off, it will be put back on for this function
    pub async fn load_file(
        &self,
        file: &RomFile,
        file_match: &FileMatch,
    ) -> Result<Option<Bytes>, Error> {
        let Some(rom_bytes) = self.load(file_match.rom_id).await? else {
            return Ok(None);
        };

        let Some(header) = file.missing_header(file_match.form) else {
            return Ok(Some(rom_bytes));
        };

        let mut combined = BytesMut::with_capacity(header.len() + rom_bytes.len());
        combined.put_slice(header);
        combined.put_slice(&rom_bytes);

        Ok(Some(combined.freeze()))
    }

    pub async fn digest(&self, id: RomId) -> Result<Option<FileDigest>, Error> {
        if let Some(digest) = self
            .digests
            .read_async(&id, |_, digest| digest.clone())
            .await
        {
            return Ok(Some(digest));
        }

        let Some(rom_bytes) = self.load(id).await? else {
            return Ok(None);
        };

        let digest =
            tokio::task::spawn_blocking(move || FileDigest::new(rom_bytes.as_ref())).await??;
        self.digests.upsert_async(id, digest.clone()).await;

        Ok(Some(digest))
    }

    pub async fn auto_generate_specification(
        &self,
        rom_id: RomId,
    ) -> Result<Option<Manifest>, Error> {
        let external_path = self
            .external_roms
            .read_async(&rom_id, |_, path| path.clone())
            .await;
        let Some(rom) = self.load(rom_id).await? else {
            return Ok(None);
        };

        let Some(machine) = SystemId::guess(external_path.as_deref(), Some(&rom)) else {
            return Ok(None);
        };

        let (file_name, name) = external_path
            .into_iter()
            .find_map(|path| {
                let file_name = path.file_name()?.to_string_lossy().to_string();
                let name = path.file_stem()?.to_string_lossy().to_string();

                Some((file_name, name))
            })
            .unwrap_or_else(|| (rom_id.to_string(), rom_id.to_string()));

        let program_id = ProgramId {
            system: machine,
            main_name: name.clone(),
        };

        let file = RomFile {
            size: rom.len() as u64,
            rom_id: Some(rom_id),
            hashes: RomHashes::default(),
            status: DumpStatus::Unverified,
            non_canonical_header: None,
        };

        Ok(Some(Manifest {
            id: program_id,
            info: ProgramInfo::V0 {
                title: Title {
                    text: name,
                    language: None,
                },
                alternative_titles: BTreeSet::default(),
                regions: BTreeSet::default(),
                languages: BTreeSet::default(),
                revision: None,
                kind: Default::default(),
                flags: BTreeSet::default(),
                video_standards: BTreeSet::default(),
                serials: BTreeSet::default(),
            },
            variant: Variant::V0 {
                files: BTreeMap::from([(file_name.clone(), file)]),
            },
            files: BTreeMap::from([(
                file_name,
                FileMatch {
                    rom_id,
                    strength: MatchStrength::Canonical,
                    form: RomForm::Canonical,
                },
            )]),
        }))
    }

    pub fn register_embedded_rom(&mut self, bytes: &'static [u8]) -> RomId {
        let rom_id = RomId::new(bytes).unwrap();

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
