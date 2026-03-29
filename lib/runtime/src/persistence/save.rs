use std::{collections::HashMap, io::Read, path::PathBuf};

use fluxemu_program::RomId;
use serde::{Deserialize, Serialize};

use crate::{
    component::{ComponentRegistry, ComponentVersion},
    path::ComponentPath,
    persistence::CompressionFormat,
};

#[derive(Debug, Serialize, Deserialize)]
pub struct Save {
    pub components: HashMap<ComponentPath, ComponentSave>,
    pub compression: Option<CompressionFormat>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ComponentSave {
    pub version: ComponentVersion,
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
        _rom_id: RomId,
        _rom_name: &str,
        _component_path: ComponentPath,
    ) -> Result<Option<(impl Read, ComponentVersion)>, Box<dyn std::error::Error>> {
        Ok(None::<(&[u8], _)>)
    }

    pub fn write(
        &self,
        _rom_id: RomId,
        _rom_name: &str,
        _registry: &ComponentRegistry,
    ) -> Result<(), Box<dyn std::error::Error>> {
        todo!()
    }
}
