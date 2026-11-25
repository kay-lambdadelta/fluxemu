use core::fmt::Display;

use serde::{Deserialize, Serialize};
use uuid::{NonNilUuid, Uuid};

pub mod hotkey;

#[derive(Serialize, Deserialize, Debug, Hash, PartialEq, Eq, PartialOrd, Ord, Clone, Copy)]
pub struct HostInputDeviceId(pub Uuid);

impl HostInputDeviceId {
    /// The ID of the platforms default input device
    ///
    /// For desktop operating systems, this is the keyboard
    ///
    /// For handheld consoles with abnormal operating systems this is the built
    /// in gamepad
    pub const PLATFORM_RESERVED: HostInputDeviceId = HostInputDeviceId(Uuid::from_u128(0));

    /// Creates a new gamepad ID
    pub const fn new(id: NonNilUuid) -> Self {
        Self(id.get())
    }
}

impl Display for HostInputDeviceId {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        self.0.fmt(f)
    }
}

#[derive(Serialize, Deserialize, Debug, Hash, PartialEq, Eq, PartialOrd, Ord, Clone, Copy)]
pub enum HostInputDeviceKind {
    Keyboard,
    Gamepad,
}

#[derive(Debug, Hash, PartialEq, Eq, PartialOrd, Ord, Clone, Copy)]
pub struct HostInputDeviceInstanceId {
    pub device: HostInputDeviceId,
    pub slot: u8,
}

impl HostInputDeviceInstanceId {
    pub const PLATFORM_RESERVED: HostInputDeviceInstanceId =
        HostInputDeviceInstanceId::new(HostInputDeviceId::PLATFORM_RESERVED, 0);

    pub const fn new(device: HostInputDeviceId, slot: u8) -> Self {
        Self { device, slot }
    }
}

impl Display for HostInputDeviceInstanceId {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{}#{}", self.device, self.slot)
    }
}
