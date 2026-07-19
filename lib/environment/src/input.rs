use std::collections::{BTreeMap, BTreeSet};

use fluxemu_input::{
    InputId,
    physical::hotkey::{Hotkey, default_hotkeys},
};
use fluxemu_program::ProgramId;
use fluxemu_runtime::ResourcePath;
use serde::{Deserialize, Serialize};

pub type InputMapping = BTreeMap<InputId, InputId>;

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct PhysicalGamepadConfiguration {
    pub hotkey: BTreeMap<BTreeSet<InputId>, Hotkey>,
    pub program_specific_mappings: BTreeMap<ProgramId, BTreeMap<ResourcePath, InputMapping>>,
}

impl Default for PhysicalGamepadConfiguration {
    fn default() -> Self {
        Self {
            hotkey: default_hotkeys().collect(),
            program_specific_mappings: BTreeMap::new(),
        }
    }
}
