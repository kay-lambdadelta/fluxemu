use alloc::collections::btree_set::BTreeSet;

use keyboard_types::Code;
use serde::{Deserialize, Serialize};
use strum::EnumIter;

use crate::{GamepadInputId, InputId};

/// Possible hotkeys this emulator could use
#[derive(Serialize, Deserialize, Debug, Copy, Clone, PartialEq, Eq, Hash, EnumIter)]
pub enum Hotkey {
    ToggleMenu,
    FastForward,
    LoadSnapshot,
    StoreSnapshot,
    IncrementSnapshotCounter,
    DecrementSnapshotCounter,
    UndoLoadSnapshot,
    UndoStoreSnapshot,
    ToggleDebugOverlay,
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
        ([InputId::Keyboard(Code::F1)].into(), Hotkey::ToggleMenu),
        (
            [
                InputId::Gamepad(GamepadInputId::Mode),
                InputId::Gamepad(GamepadInputId::Select),
            ]
            .into(),
            Hotkey::FastForward,
        ),
        ([InputId::Keyboard(Code::F2)].into(), Hotkey::FastForward),
        (
            [
                InputId::Gamepad(GamepadInputId::Mode),
                InputId::Gamepad(GamepadInputId::DPadLeft),
            ]
            .into(),
            Hotkey::StoreSnapshot,
        ),
        ([InputId::Keyboard(Code::F3)].into(), Hotkey::StoreSnapshot),
        (
            [
                InputId::Gamepad(GamepadInputId::Mode),
                InputId::Gamepad(GamepadInputId::DPadRight),
            ]
            .into(),
            Hotkey::LoadSnapshot,
        ),
        ([InputId::Keyboard(Code::F4)].into(), Hotkey::LoadSnapshot),
        (
            [
                InputId::Gamepad(GamepadInputId::Mode),
                InputId::Gamepad(GamepadInputId::DPadUp),
            ]
            .into(),
            Hotkey::IncrementSnapshotCounter,
        ),
        (
            [InputId::Keyboard(Code::F5)].into(),
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
            [InputId::Keyboard(Code::F6)].into(),
            Hotkey::DecrementSnapshotCounter,
        ),
        (
            [
                InputId::Gamepad(GamepadInputId::Mode),
                InputId::Gamepad(GamepadInputId::FPadRight),
            ]
            .into(),
            Hotkey::UndoLoadSnapshot,
        ),
        (
            [InputId::Keyboard(Code::F7)].into(),
            Hotkey::UndoLoadSnapshot,
        ),
        (
            [
                InputId::Gamepad(GamepadInputId::Mode),
                InputId::Gamepad(GamepadInputId::FPadLeft),
            ]
            .into(),
            Hotkey::UndoStoreSnapshot,
        ),
        (
            [InputId::Keyboard(Code::F8)].into(),
            Hotkey::UndoStoreSnapshot,
        ),
        (
            [
                InputId::Gamepad(GamepadInputId::Mode),
                InputId::Gamepad(GamepadInputId::FPadUp),
            ]
            .into(),
            Hotkey::ToggleDebugOverlay,
        ),
        (
            [InputId::Keyboard(Code::F9)].into(),
            Hotkey::ToggleDebugOverlay,
        ),
    ]
    .into_iter()
}
