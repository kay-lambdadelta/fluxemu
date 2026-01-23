use std::collections::HashMap;

use fluxemu_runtime::{
    input::{Input, RealGamepadId},
    path::FluxEmuPath,
    program::MachineId,
};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Default)]
/// Mappings between a real physical gamepad and a virtual one registered by an component
pub struct Real2VirtualMappings(pub HashMap<RealGamepadId, HashMap<Input, Input>>);

#[derive(Serialize, Deserialize, Debug, Default)]
/// Configuration for gamepads
pub struct GamepadConfigs {
    /// TODO: is machine id and path unique enough?
    pub gamepads: HashMap<(MachineId, FluxEmuPath), Real2VirtualMappings>,
}
