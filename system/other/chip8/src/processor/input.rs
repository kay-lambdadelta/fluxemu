use fluxemu_input::{InputId, KeyboardInputId};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Deserialize, Serialize)]
pub(super) struct Chip8KeyCode(pub u8);

impl TryFrom<InputId> for Chip8KeyCode {
    type Error = ();

    fn try_from(value: InputId) -> Result<Self, Self::Error> {
        match value {
            InputId::Keyboard(KeyboardInputId::Numpad0) => Ok(Chip8KeyCode(0x0)),
            InputId::Keyboard(KeyboardInputId::Numpad1) => Ok(Chip8KeyCode(0x1)),
            InputId::Keyboard(KeyboardInputId::Numpad2) => Ok(Chip8KeyCode(0x2)),
            InputId::Keyboard(KeyboardInputId::Numpad3) => Ok(Chip8KeyCode(0x3)),
            InputId::Keyboard(KeyboardInputId::Numpad4) => Ok(Chip8KeyCode(0x4)),
            InputId::Keyboard(KeyboardInputId::Numpad5) => Ok(Chip8KeyCode(0x5)),
            InputId::Keyboard(KeyboardInputId::Numpad6) => Ok(Chip8KeyCode(0x6)),
            InputId::Keyboard(KeyboardInputId::Numpad7) => Ok(Chip8KeyCode(0x7)),
            InputId::Keyboard(KeyboardInputId::Numpad8) => Ok(Chip8KeyCode(0x8)),
            InputId::Keyboard(KeyboardInputId::Numpad9) => Ok(Chip8KeyCode(0x9)),
            InputId::Keyboard(KeyboardInputId::KeyA) => Ok(Chip8KeyCode(0xa)),
            InputId::Keyboard(KeyboardInputId::KeyB) => Ok(Chip8KeyCode(0xb)),
            InputId::Keyboard(KeyboardInputId::KeyC) => Ok(Chip8KeyCode(0xc)),
            InputId::Keyboard(KeyboardInputId::KeyD) => Ok(Chip8KeyCode(0xd)),
            InputId::Keyboard(KeyboardInputId::KeyE) => Ok(Chip8KeyCode(0xe)),
            InputId::Keyboard(KeyboardInputId::KeyF) => Ok(Chip8KeyCode(0xf)),
            _ => Err(()),
        }
    }
}

impl TryFrom<Chip8KeyCode> for InputId {
    type Error = ();

    fn try_from(value: Chip8KeyCode) -> Result<Self, Self::Error> {
        match value.0 {
            0x0 => Ok(InputId::Keyboard(KeyboardInputId::Numpad0)),
            0x1 => Ok(InputId::Keyboard(KeyboardInputId::Numpad1)),
            0x2 => Ok(InputId::Keyboard(KeyboardInputId::Numpad2)),
            0x3 => Ok(InputId::Keyboard(KeyboardInputId::Numpad3)),
            0x4 => Ok(InputId::Keyboard(KeyboardInputId::Numpad4)),
            0x5 => Ok(InputId::Keyboard(KeyboardInputId::Numpad5)),
            0x6 => Ok(InputId::Keyboard(KeyboardInputId::Numpad6)),
            0x7 => Ok(InputId::Keyboard(KeyboardInputId::Numpad7)),
            0x8 => Ok(InputId::Keyboard(KeyboardInputId::Numpad8)),
            0x9 => Ok(InputId::Keyboard(KeyboardInputId::Numpad9)),
            0xa => Ok(InputId::Keyboard(KeyboardInputId::KeyA)),
            0xb => Ok(InputId::Keyboard(KeyboardInputId::KeyB)),
            0xc => Ok(InputId::Keyboard(KeyboardInputId::KeyC)),
            0xd => Ok(InputId::Keyboard(KeyboardInputId::KeyD)),
            0xe => Ok(InputId::Keyboard(KeyboardInputId::KeyE)),
            0xf => Ok(InputId::Keyboard(KeyboardInputId::KeyF)),
            _ => Err(()),
        }
    }
}

pub const DEFAULT_MAPPINGS: [(InputId, InputId); 16] = [
    // Keyboard mappings
    (
        InputId::Keyboard(KeyboardInputId::Digit1),
        InputId::Keyboard(KeyboardInputId::Numpad1),
    ),
    (
        InputId::Keyboard(KeyboardInputId::Digit2),
        InputId::Keyboard(KeyboardInputId::Numpad2),
    ),
    (
        InputId::Keyboard(KeyboardInputId::Digit3),
        InputId::Keyboard(KeyboardInputId::Numpad3),
    ),
    (
        InputId::Keyboard(KeyboardInputId::Digit4),
        InputId::Keyboard(KeyboardInputId::KeyC),
    ),
    (
        InputId::Keyboard(KeyboardInputId::KeyQ),
        InputId::Keyboard(KeyboardInputId::Numpad4),
    ),
    (
        InputId::Keyboard(KeyboardInputId::KeyW),
        InputId::Keyboard(KeyboardInputId::Numpad5),
    ),
    (
        InputId::Keyboard(KeyboardInputId::KeyE),
        InputId::Keyboard(KeyboardInputId::Numpad6),
    ),
    (
        InputId::Keyboard(KeyboardInputId::KeyR),
        InputId::Keyboard(KeyboardInputId::KeyD),
    ),
    (
        InputId::Keyboard(KeyboardInputId::KeyA),
        InputId::Keyboard(KeyboardInputId::Numpad7),
    ),
    (
        InputId::Keyboard(KeyboardInputId::KeyS),
        InputId::Keyboard(KeyboardInputId::Numpad8),
    ),
    (
        InputId::Keyboard(KeyboardInputId::KeyD),
        InputId::Keyboard(KeyboardInputId::Numpad9),
    ),
    (
        InputId::Keyboard(KeyboardInputId::KeyF),
        InputId::Keyboard(KeyboardInputId::KeyE),
    ),
    (
        InputId::Keyboard(KeyboardInputId::KeyZ),
        InputId::Keyboard(KeyboardInputId::KeyA),
    ),
    (
        InputId::Keyboard(KeyboardInputId::KeyX),
        InputId::Keyboard(KeyboardInputId::Numpad0),
    ),
    (
        InputId::Keyboard(KeyboardInputId::KeyC),
        InputId::Keyboard(KeyboardInputId::KeyB),
    ),
    (
        InputId::Keyboard(KeyboardInputId::KeyV),
        InputId::Keyboard(KeyboardInputId::KeyF),
    ),
];

pub const PRESENT_INPUTS: [InputId; 16] = [
    // Interpreting the numbers on the chip8 keypad as "numpad"
    InputId::Keyboard(KeyboardInputId::Numpad1),
    InputId::Keyboard(KeyboardInputId::Numpad2),
    InputId::Keyboard(KeyboardInputId::Numpad3),
    InputId::Keyboard(KeyboardInputId::KeyC),
    InputId::Keyboard(KeyboardInputId::Numpad4),
    InputId::Keyboard(KeyboardInputId::Numpad5),
    InputId::Keyboard(KeyboardInputId::Numpad6),
    InputId::Keyboard(KeyboardInputId::KeyD),
    InputId::Keyboard(KeyboardInputId::Numpad7),
    InputId::Keyboard(KeyboardInputId::Numpad8),
    InputId::Keyboard(KeyboardInputId::Numpad9),
    InputId::Keyboard(KeyboardInputId::KeyE),
    InputId::Keyboard(KeyboardInputId::KeyA),
    InputId::Keyboard(KeyboardInputId::Numpad0),
    InputId::Keyboard(KeyboardInputId::KeyB),
    InputId::Keyboard(KeyboardInputId::KeyF),
];
