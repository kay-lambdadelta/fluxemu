use std::collections::{BTreeMap, BTreeSet};

use fluxemu_input::{InputId, physical::hotkey::Hotkey};
use fluxemu_program::ProgramId;
use fluxemu_runtime::path::ResourcePath;
use serde::{Deserialize, Serialize};

pub type InputMapping = BTreeMap<InputId, InputId>;

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct PhysicalGamepadConfiguration {
    pub hotkey_overrides: Option<BTreeMap<BTreeSet<InputId>, Hotkey>>,
    pub program_overrides: BTreeMap<ProgramId, BTreeMap<ResourcePath, InputMapping>>,
}
