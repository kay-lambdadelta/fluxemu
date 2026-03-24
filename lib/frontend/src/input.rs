use std::{borrow::Cow, collections::HashSet};

use fluxemu_input::InputId;

#[derive(Debug, Clone)]
/// Information a component gave about a emulated gamepad
pub struct PhysicalInputDeviceMetadata {
    pub name: Cow<'static, str>,
    pub present_inputs: HashSet<InputId>,
}
