use std::collections::{HashMap, HashSet};

use fluxemu_input::{InputId, InputState};
use rustc_hash::FxBuildHasher;

use crate::path::ResourcePath;

/// Input state for a components registered input device
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

    /// Returns a reference to the device's metadata that the component registered it with
    pub fn metadata(&self) -> &LogicalInputDeviceMetadata {
        &self.metadata
    }

    /// Sets the state of an input on this device
    ///
    /// # Panics
    ///
    /// If the component never registered the passed in input id
    pub fn set_state(&self, input_id: InputId, state: InputState) {
        self.state
            .update_sync(&input_id, |_, state_ref| *state_ref = state)
            .unwrap_or_else(|| panic!("Input ID not found in device metadata: {:?}", input_id));
    }

    /// Returns the state of an input on this device
    ///
    /// # Panics
    ///
    /// If the component never registered the passed in input id
    pub fn get_state(&self, input_id: InputId) -> InputState {
        self.state
            .read_sync(&input_id, |_, input_state| *input_state)
            .unwrap_or_else(|| panic!("Input ID not found in device metadata: {:?}", input_id))
    }
}

/// Metadata for a logical input device
#[derive(Debug)]
pub struct LogicalInputDeviceMetadata {
    /// A stable path for this device
    pub path: ResourcePath,
    /// The inputs that this device could accept
    ///
    /// Any other inputs will be rejected by the runtime
    pub present_inputs: HashSet<InputId, FxBuildHasher>,
    /// Some "good" default mappings the frontends and shells can use for intuitive gameplay for players who don't rebind anything
    pub default_mappings: HashMap<InputId, InputId, FxBuildHasher>,
}
