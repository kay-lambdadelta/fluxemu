use std::{collections::HashMap, io::Read, path::PathBuf};

use fluxemu_program::ProgramId;
use serde::{Deserialize, Serialize};

use crate::{
    component::ComponentRegistry,
    path::ComponentPath,
    persistence::{CompressionFormat, PersistanceFormatVersion},
};

pub struct Save {
    metadata: Metadata,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Metadata {
    pub version: u32,
    pub components: HashMap<ComponentPath, ComponentMetadata>,
    pub compression: Option<CompressionFormat>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ComponentMetadata {
    version: PersistanceFormatVersion,
}

#[allow(unused)]
#[derive(Debug)]
pub struct SaveManager {
    save_directory: Option<PathBuf>,
}

impl SaveManager {
    pub fn new(save_directory: Option<PathBuf>) -> Self {
        Self { save_directory }
    }

    pub fn get(
        &self,
        _program_id: &ProgramId,
        _component_path: ComponentPath,
    ) -> Result<Option<(impl Read, PersistanceFormatVersion)>, Box<dyn std::error::Error>> {
        Ok(None::<(&[u8], _)>)
    }

    pub fn write(
        &self,
        _program_id: &ProgramId,
        _registry: &ComponentRegistry,
    ) -> Result<(), Box<dyn std::error::Error>> {
        todo!()
    }
}
