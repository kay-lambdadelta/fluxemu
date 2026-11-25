use std::collections::{BTreeMap, BTreeSet};

use fluxemu_input::{
    InputId,
    host::hotkey::{Hotkey, default_hotkeys},
};
use fluxemu_program::ProgramId;
use fluxemu_runtime::ResourcePath;
use ron::value::RawValue;
use serde::{Deserialize, Serialize};

pub type InputMapping = BTreeMap<InputId, Box<RawValue>>;

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct HostGamepadConfiguration {
    pub hotkey: BTreeMap<BTreeSet<InputId>, Hotkey>,
    pub program_specific_mappings: BTreeMap<ProgramId, BTreeMap<ResourcePath, InputMapping>>,
}

impl Default for HostGamepadConfiguration {
    fn default() -> Self {
        Self {
            hotkey: default_hotkeys().collect(),
            program_specific_mappings: BTreeMap::new(),
        }
    }
}
