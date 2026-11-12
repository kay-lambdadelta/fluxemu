#![no_std]

extern crate alloc;

use keyboard_types::Code;
use serde::{Deserialize, Serialize};

/// Virtual gamepad
mod gamepad;

pub mod physical;

pub use gamepad::*;

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
/// Enum covering all possible input types
pub enum InputId {
    /// Input is for a gamepad device
    Gamepad(GamepadInputId),
    /// Input is for a keyboard device
    Keyboard(Code),
}

#[derive(Debug, Copy, Clone, PartialEq, Default)]
pub struct InputState(f32);

impl InputState {
    /// Digital press
    pub const PRESSED: Self = Self(1.0);
    /// Digital release
    pub const RELEASED: Self = Self(0.0);

    pub fn new(value: f32) -> Self {
        Self(value.clamp(0.0, 1.0))
    }

    pub fn as_analog(&self) -> f32 {
        self.0
    }

    /// Interprets self as a digital input
    pub fn as_digital(&self, threshhold: Option<f32>) -> bool {
        let threshhold = threshhold.unwrap_or(0.5);

        self.0 >= threshhold
    }
}
