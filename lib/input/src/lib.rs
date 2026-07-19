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
    /// Input is for a gamepad device
    Gamepad(GamepadInputId),
    /// Input is for a keyboard device
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

#[derive(Debug, Copy, Clone, PartialEq, Default)]
pub struct InputState(pub f32);

impl InputState {
    /// Digital press
    pub const PRESSED: Self = Self(1.0);
    /// Digital release
    pub const RELEASED: Self = Self(0.0);

    /// Interprets self as a digital input
    pub fn as_digital(&self, threshhold: Option<f32>) -> bool {
        let threshhold = threshhold.unwrap_or(0.5);

        self.0 >= threshhold
    }
}
