use std::{collections::HashMap, io::Read, path::PathBuf};

use fluxemu_program::RomId;
use serde::{Deserialize, Serialize};

use crate::{
    component::ComponentRegistry,
    path::ComponentPath,
    persistence::{CompressionFormat, PersistanceFormatVersion},
};

pub type SnapshotSlot = u16;

#[derive(Debug, Serialize, Deserialize)]
pub struct SnapshotMetadata {
    pub components: HashMap<ComponentPath, ComponentSnapshotInfo>,
    pub compression: Option<CompressionFormat>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ComponentSnapshotInfo {
    pub version: PersistanceFormatVersion,
}

#[allow(unused)]
#[derive(Debug)]
pub struct SnapshotManager {
    snapshot_directory: Option<PathBuf>,
}

impl SnapshotManager {
    pub fn new(snapshot_directory: Option<PathBuf>) -> Self {
        Self { snapshot_directory }
    }

    pub fn read(
        &self,
        _rom_id: RomId,
        _rom_name: &str,
        _slot: SnapshotSlot,
        _registry: &ComponentRegistry,
    ) -> Result<Option<(impl Read, PersistanceFormatVersion)>, Box<dyn std::error::Error>> {
        Ok(None::<(&[u8], _)>)
    }

    pub fn write(
        &self,
        _rom_id: RomId,
        _rom_name: &str,
        _slot: SnapshotSlot,
        _registry: &ComponentRegistry,
    ) -> Result<(), Box<dyn std::error::Error>> {
        todo!()
    }
}
