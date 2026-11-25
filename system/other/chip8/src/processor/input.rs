use fluxemu_input::InputId;
use fluxemu_runtime::input::GuestInput;
use keyboard_types::Code;
use serde::{Deserialize, Serialize};
use strum::{EnumIter, FromRepr, IntoEnumIterator};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Deserialize, Serialize)]
pub(super) struct Chip8KeyCode(pub u8);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, EnumIter, FromRepr)]
#[repr(u8)]
pub enum Chip8Key {
    Key0,
    Key1,
    Key2,
    Key3,
    Key4,
    Key5,
    Key6,
    Key7,
    Key8,
    Key9,
    KeyA,
    KeyB,
    KeyC,
    KeyD,
    KeyE,
    KeyF,
}

impl GuestInput for Chip8Key {
    fn all() -> impl Iterator<Item = Self> {
        Self::iter()
    }
}

impl TryFrom<Chip8KeyCode> for Chip8Key {
    type Error = ();

    fn try_from(value: Chip8KeyCode) -> Result<Self, Self::Error> {
        Self::from_repr(value.0).ok_or(())
    }
}

pub const DEFAULT_MAPPINGS: [(InputId, Chip8Key); 16] = [
    (InputId::Keyboard(Code::Digit1), Chip8Key::Key1),
    (InputId::Keyboard(Code::Digit2), Chip8Key::Key2),
    (InputId::Keyboard(Code::Digit3), Chip8Key::Key3),
    (InputId::Keyboard(Code::Digit4), Chip8Key::KeyC),
    (InputId::Keyboard(Code::KeyQ), Chip8Key::Key4),
    (InputId::Keyboard(Code::KeyW), Chip8Key::Key5),
    (InputId::Keyboard(Code::KeyE), Chip8Key::Key6),
    (InputId::Keyboard(Code::KeyR), Chip8Key::KeyD),
    (InputId::Keyboard(Code::KeyA), Chip8Key::Key7),
    (InputId::Keyboard(Code::KeyS), Chip8Key::Key8),
    (InputId::Keyboard(Code::KeyD), Chip8Key::Key9),
    (InputId::Keyboard(Code::KeyF), Chip8Key::KeyE),
    (InputId::Keyboard(Code::KeyZ), Chip8Key::KeyA),
    (InputId::Keyboard(Code::KeyX), Chip8Key::Key0),
    (InputId::Keyboard(Code::KeyC), Chip8Key::KeyB),
    (InputId::Keyboard(Code::KeyV), Chip8Key::KeyF),
];
