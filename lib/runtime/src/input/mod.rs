use std::collections::{HashMap, HashSet};

use fluxemu_input::{InputId, InputState};
use rustc_hash::FxBuildHasher;

use crate::path::ResourcePath;

#[derive(Debug)]
pub struct LogicalInputDevice {
    state: scc::HashMap<InputId, InputState>,
    metadata: LogicalInputDeviceMetadata,
}

impl LogicalInputDevice {
    pub(crate) fn new(metadata: LogicalInputDeviceMetadata) -> Self {
        Self {
            state: scc::HashMap::from_iter(
                metadata
                    .present_inputs
                    .iter()
                    .copied()
                    .map(|input_id| (input_id, InputState::RELEASED)),
            ),
            metadata,
        }
    }

    pub fn metadata(&self) -> &LogicalInputDeviceMetadata {
        &self.metadata
    }

    pub fn set_state(&self, input_id: InputId, state: InputState) {
        self.state
            .update_sync(&input_id, |_, state_ref| *state_ref = state)
            .unwrap_or_else(|| panic!("Input ID not found in device metadata: {:?}", input_id));
    }

    pub fn get_state(&self, input_id: InputId) -> InputState {
        self.state
            .read_sync(&input_id, |_, input_state| *input_state)
            .unwrap_or_else(|| panic!("Input ID not found in device metadata: {:?}", input_id))
    }
}

#[derive(Debug)]
pub struct LogicalInputDeviceMetadata {
    pub path: ResourcePath,
    pub present_inputs: HashSet<InputId, FxBuildHasher>,
    pub default_mappings: HashMap<InputId, InputId, FxBuildHasher>,
}
