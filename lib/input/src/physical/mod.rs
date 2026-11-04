use core::fmt::Display;

use serde::{Deserialize, Serialize};
use uuid::{NonNilUuid, Uuid};

pub mod hotkey;

#[derive(Serialize, Deserialize, Debug, Hash, PartialEq, Eq, PartialOrd, Ord, Clone, Copy)]
pub struct PhysicalInputDeviceId(pub Uuid);

impl PhysicalInputDeviceId {
    /// The ID of the platforms default input device
    ///
    /// For desktop operating systems, this is the keyboard
    ///
    /// For handheld consoles with abnormal operating systems this is the built
    /// in gamepad
    pub const PLATFORM_RESERVED: PhysicalInputDeviceId = PhysicalInputDeviceId(Uuid::from_u128(0));

    /// Creates a new gamepad ID
    pub const fn new(id: NonNilUuid) -> Self {
        Self(id.get())
    }
}

impl Display for PhysicalInputDeviceId {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        self.0.fmt(f)
    }
}

#[derive(Serialize, Deserialize, Debug, Hash, PartialEq, Eq, PartialOrd, Ord, Clone, Copy)]
pub enum PhysicalInputDeviceKind {
    Keyboard,
    Gamepad,
}

#[derive(Debug, Hash, PartialEq, Eq, PartialOrd, Ord, Clone, Copy)]
pub struct PhysicalInputDeviceInstanceId {
    pub device: PhysicalInputDeviceId,
    pub slot: u8,
}

impl PhysicalInputDeviceInstanceId {
    pub const PLATFORM_RESERVED: PhysicalInputDeviceInstanceId =
        PhysicalInputDeviceInstanceId::new(PhysicalInputDeviceId::PLATFORM_RESERVED, 0);

    pub const fn new(device: PhysicalInputDeviceId, slot: u8) -> Self {
        Self { device, slot }
    }
}

impl Display for PhysicalInputDeviceInstanceId {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{}#{}", self.device, self.slot)
    }
}
