use keyboard_types::Code;
use serde::{Deserialize, Serialize};

/// Virtual gamepad
mod gamepad;

pub mod host;
mod state;

pub use gamepad::*;
pub use state::*;

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
/// Enum covering all possible input types
pub enum InputId {
    /// Input is for a gamepad device
    Gamepad(GamepadInputId),
    /// Input is for a keyboard device
    Keyboard(Code),
}

impl InputId {
    pub fn shape(&self) -> InputShape {
        match self {
            Self::Gamepad(input) => input.shape(),
            Self::Keyboard(_) => InputShape::Button,
        }
    }

    pub fn split(self) -> (Self, DirectionPart) {
        match self {
            Self::Gamepad(input) => {
                let (whole, part) = input.split();

                (Self::Gamepad(whole), part)
            }
            Self::Keyboard(_) => (self, DirectionPart::All),
        }
    }
}
