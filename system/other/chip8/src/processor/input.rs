use fluxemu_input::InputId;
use keyboard_types::Code;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Deserialize, Serialize)]
pub(super) struct Chip8KeyCode(pub u8);

impl TryFrom<InputId> for Chip8KeyCode {
    type Error = ();

    fn try_from(value: InputId) -> Result<Self, Self::Error> {
        match value {
            InputId::Keyboard(Code::Numpad0) => Ok(Chip8KeyCode(0x0)),
            InputId::Keyboard(Code::Numpad1) => Ok(Chip8KeyCode(0x1)),
            InputId::Keyboard(Code::Numpad2) => Ok(Chip8KeyCode(0x2)),
            InputId::Keyboard(Code::Numpad3) => Ok(Chip8KeyCode(0x3)),
            InputId::Keyboard(Code::Numpad4) => Ok(Chip8KeyCode(0x4)),
            InputId::Keyboard(Code::Numpad5) => Ok(Chip8KeyCode(0x5)),
            InputId::Keyboard(Code::Numpad6) => Ok(Chip8KeyCode(0x6)),
            InputId::Keyboard(Code::Numpad7) => Ok(Chip8KeyCode(0x7)),
            InputId::Keyboard(Code::Numpad8) => Ok(Chip8KeyCode(0x8)),
            InputId::Keyboard(Code::Numpad9) => Ok(Chip8KeyCode(0x9)),
            InputId::Keyboard(Code::KeyA) => Ok(Chip8KeyCode(0xa)),
            InputId::Keyboard(Code::KeyB) => Ok(Chip8KeyCode(0xb)),
            InputId::Keyboard(Code::KeyC) => Ok(Chip8KeyCode(0xc)),
            InputId::Keyboard(Code::KeyD) => Ok(Chip8KeyCode(0xd)),
            InputId::Keyboard(Code::KeyE) => Ok(Chip8KeyCode(0xe)),
            InputId::Keyboard(Code::KeyF) => Ok(Chip8KeyCode(0xf)),
            _ => Err(()),
        }
    }
}

impl TryFrom<Chip8KeyCode> for InputId {
    type Error = ();

    fn try_from(value: Chip8KeyCode) -> Result<Self, Self::Error> {
        match value.0 {
            0x0 => Ok(InputId::Keyboard(Code::Numpad0)),
            0x1 => Ok(InputId::Keyboard(Code::Numpad1)),
            0x2 => Ok(InputId::Keyboard(Code::Numpad2)),
            0x3 => Ok(InputId::Keyboard(Code::Numpad3)),
            0x4 => Ok(InputId::Keyboard(Code::Numpad4)),
            0x5 => Ok(InputId::Keyboard(Code::Numpad5)),
            0x6 => Ok(InputId::Keyboard(Code::Numpad6)),
            0x7 => Ok(InputId::Keyboard(Code::Numpad7)),
            0x8 => Ok(InputId::Keyboard(Code::Numpad8)),
            0x9 => Ok(InputId::Keyboard(Code::Numpad9)),
            0xa => Ok(InputId::Keyboard(Code::KeyA)),
            0xb => Ok(InputId::Keyboard(Code::KeyB)),
            0xc => Ok(InputId::Keyboard(Code::KeyC)),
            0xd => Ok(InputId::Keyboard(Code::KeyD)),
            0xe => Ok(InputId::Keyboard(Code::KeyE)),
            0xf => Ok(InputId::Keyboard(Code::KeyF)),
            _ => Err(()),
        }
    }
}

pub const DEFAULT_MAPPINGS: [(InputId, InputId); 16] = [
    // Keyboard mappings
    (
        InputId::Keyboard(Code::Digit1),
        InputId::Keyboard(Code::Numpad1),
    ),
    (
        InputId::Keyboard(Code::Digit2),
        InputId::Keyboard(Code::Numpad2),
    ),
    (
        InputId::Keyboard(Code::Digit3),
        InputId::Keyboard(Code::Numpad3),
    ),
    (
        InputId::Keyboard(Code::Digit4),
        InputId::Keyboard(Code::KeyC),
    ),
    (
        InputId::Keyboard(Code::KeyQ),
        InputId::Keyboard(Code::Numpad4),
    ),
    (
        InputId::Keyboard(Code::KeyW),
        InputId::Keyboard(Code::Numpad5),
    ),
    (
        InputId::Keyboard(Code::KeyE),
        InputId::Keyboard(Code::Numpad6),
    ),
    (InputId::Keyboard(Code::KeyR), InputId::Keyboard(Code::KeyD)),
    (
        InputId::Keyboard(Code::KeyA),
        InputId::Keyboard(Code::Numpad7),
    ),
    (
        InputId::Keyboard(Code::KeyS),
        InputId::Keyboard(Code::Numpad8),
    ),
    (
        InputId::Keyboard(Code::KeyD),
        InputId::Keyboard(Code::Numpad9),
    ),
    (InputId::Keyboard(Code::KeyF), InputId::Keyboard(Code::KeyE)),
    (InputId::Keyboard(Code::KeyZ), InputId::Keyboard(Code::KeyA)),
    (
        InputId::Keyboard(Code::KeyX),
        InputId::Keyboard(Code::Numpad0),
    ),
    (InputId::Keyboard(Code::KeyC), InputId::Keyboard(Code::KeyB)),
    (InputId::Keyboard(Code::KeyV), InputId::Keyboard(Code::KeyF)),
];

pub const PRESENT_INPUTS: [InputId; 16] = [
    // Interpreting the numbers on the chip8 keypad as "numpad"
    InputId::Keyboard(Code::Numpad1),
    InputId::Keyboard(Code::Numpad2),
    InputId::Keyboard(Code::Numpad3),
    InputId::Keyboard(Code::KeyC),
    InputId::Keyboard(Code::Numpad4),
    InputId::Keyboard(Code::Numpad5),
    InputId::Keyboard(Code::Numpad6),
    InputId::Keyboard(Code::KeyD),
    InputId::Keyboard(Code::Numpad7),
    InputId::Keyboard(Code::Numpad8),
    InputId::Keyboard(Code::Numpad9),
    InputId::Keyboard(Code::KeyE),
    InputId::Keyboard(Code::KeyA),
    InputId::Keyboard(Code::Numpad0),
    InputId::Keyboard(Code::KeyB),
    InputId::Keyboard(Code::KeyF),
];
