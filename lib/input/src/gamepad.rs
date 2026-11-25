use serde::{Deserialize, Serialize};
use strum::EnumIter;

use crate::{DirectionPart, InputShape};

/// Inputs that a gamepad could have
#[non_exhaustive]
#[derive(
    Serialize, Deserialize, Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, EnumIter,
)]
pub enum GamepadInputId {
    FPadUp,
    FPadDown,
    FPadLeft,
    FPadRight,
    CPadUp,
    CPadDown,
    CPadLeft,
    CPadRight,
    Select,
    Start,
    Mode,
    LeftThumb,
    RightThumb,
    DPad(DirectionPart),
    LeftTrigger,
    RightTrigger,
    ZTrigger,
    LeftSecondaryTrigger,
    RightSecondaryTrigger,
    LeftStick(DirectionPart),
    RightStick(DirectionPart),
}

impl GamepadInputId {
    pub fn shape(&self) -> InputShape {
        match self {
            Self::DPad(_) | Self::LeftStick(_) | Self::RightStick(_) => InputShape::Direction,
            _ => InputShape::Button,
        }
    }

    pub fn split(self) -> (Self, DirectionPart) {
        match self {
            Self::DPad(part) => (Self::DPad(DirectionPart::All), part),
            Self::LeftStick(part) => (Self::LeftStick(DirectionPart::All), part),
            Self::RightStick(part) => (Self::RightStick(DirectionPart::All), part),
            whole => (whole, DirectionPart::All),
        }
    }

    pub fn with_part(self, part: DirectionPart) -> Self {
        match self {
            Self::DPad(_) => Self::DPad(part),
            Self::LeftStick(_) => Self::LeftStick(part),
            Self::RightStick(_) => Self::RightStick(part),
            whole => whole,
        }
    }
}
