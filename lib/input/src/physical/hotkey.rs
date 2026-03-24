use alloc::collections::btree_set::BTreeSet;

use serde::{Deserialize, Serialize};
use strum::EnumIter;

use crate::{GamepadInputId, InputId, KeyboardInputId};

#[derive(Serialize, Deserialize, Debug, Copy, Clone, PartialEq, Eq, Hash, EnumIter)]
/// Possible hotkeys this emulator could use
#[allow(missing_docs)]
pub enum Hotkey {
    ToggleMenu,
    FastForward,
    LoadSnapshot,
    StoreSnapshot,
    IncrementSnapshotCounter,
    DecrementSnapshotCounter,
}

pub fn default_hotkeys() -> impl Iterator<Item = (BTreeSet<InputId>, Hotkey)> {
    [
        (
            [
                InputId::Gamepad(GamepadInputId::Mode),
                InputId::Gamepad(GamepadInputId::Start),
            ]
            .into(),
            Hotkey::ToggleMenu,
        ),
        (
            [InputId::Keyboard(KeyboardInputId::F1)].into(),
            Hotkey::ToggleMenu,
        ),
        (
            [
                InputId::Gamepad(GamepadInputId::Mode),
                InputId::Gamepad(GamepadInputId::Select),
            ]
            .into(),
            Hotkey::FastForward,
        ),
        (
            [InputId::Keyboard(KeyboardInputId::F2)].into(),
            Hotkey::FastForward,
        ),
        (
            [
                InputId::Gamepad(GamepadInputId::Mode),
                InputId::Gamepad(GamepadInputId::DPadLeft),
            ]
            .into(),
            Hotkey::StoreSnapshot,
        ),
        (
            [InputId::Keyboard(KeyboardInputId::F3)].into(),
            Hotkey::StoreSnapshot,
        ),
        (
            [
                InputId::Gamepad(GamepadInputId::Mode),
                InputId::Gamepad(GamepadInputId::DPadRight),
            ]
            .into(),
            Hotkey::LoadSnapshot,
        ),
        (
            [InputId::Keyboard(KeyboardInputId::F4)].into(),
            Hotkey::LoadSnapshot,
        ),
        (
            [
                InputId::Gamepad(GamepadInputId::Mode),
                InputId::Gamepad(GamepadInputId::DPadUp),
            ]
            .into(),
            Hotkey::IncrementSnapshotCounter,
        ),
        (
            [InputId::Keyboard(KeyboardInputId::F5)].into(),
            Hotkey::IncrementSnapshotCounter,
        ),
        (
            [
                InputId::Gamepad(GamepadInputId::Mode),
                InputId::Gamepad(GamepadInputId::DPadDown),
            ]
            .into(),
            Hotkey::DecrementSnapshotCounter,
        ),
        (
            [InputId::Keyboard(KeyboardInputId::F6)].into(),
            Hotkey::DecrementSnapshotCounter,
        ),
    ]
    .into_iter()
}
