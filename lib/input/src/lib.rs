#![no_std]

extern crate alloc;

use serde::{Deserialize, Serialize};
use strum::IntoEnumIterator;

/// Virtual gamepad
mod gamepad;
/// Keyboard enums
mod keyboard;

pub mod physical;

pub use gamepad::*;
pub use keyboard::*;

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
/// Enum covering all possible input types
pub enum InputId {
    /// Input is for a gamepadish device
    Gamepad(GamepadInputId),
    /// Input is for a keyboardish device
    Keyboard(KeyboardInputId),
}

impl InputId {
    /// Iterate over every possible input
    pub fn iter() -> impl Iterator<Item = Self> {
        GamepadInputId::iter()
            .map(InputId::Gamepad)
            .chain(KeyboardInputId::iter().map(InputId::Keyboard))
    }
}

#[derive(Debug, Copy, Clone, PartialEq)]
/// Represents the state as collected of a single input
pub enum InputState {
    /// 0 or 1
    Digital(bool),
    /// Clamped from 0.0 to 1.0
    Analog(f32),
}

impl Default for InputState {
    fn default() -> Self {
        Self::Digital(false)
    }
}

impl InputState {
    /// Digital press
    pub const PRESSED: Self = Self::Digital(true);

    /// Digital release
    pub const RELEASED: Self = Self::Digital(false);

    /// Interprets self as a digital input
    pub fn as_digital(&self, threshhold: Option<f32>) -> bool {
        match self {
            InputState::Digital(value) => *value,
            InputState::Analog(value) => *value >= threshhold.unwrap_or(0.5),
        }
    }

    /// Interprets self as an analog input
    pub fn as_analog(&self) -> f32 {
        match self {
            InputState::Digital(value) => {
                if *value {
                    1.0
                } else {
                    0.0
                }
            }
            InputState::Analog(value) => *value,
        }
    }
}
