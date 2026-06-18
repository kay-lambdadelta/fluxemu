use std::collections::HashMap;

use fluxemu_input::{GamepadInputId, InputId, InputState, physical::PhysicalInputDeviceId};
use gilrs::{Axis, Button, Gamepad, GamepadId};
use uuid::Uuid;

pub fn gilrs_button2input(button: Button) -> Option<InputId> {
    Some(InputId::Gamepad(match button {
        Button::South => GamepadInputId::FPadDown,
        Button::East => GamepadInputId::FPadRight,
        Button::North => GamepadInputId::FPadUp,
        Button::West => GamepadInputId::FPadLeft,
        Button::C => todo!(),
        Button::Z => GamepadInputId::ZTrigger,
        Button::LeftTrigger => GamepadInputId::LeftTrigger,
        Button::LeftTrigger2 => GamepadInputId::LeftSecondaryTrigger,
        Button::RightTrigger => GamepadInputId::RightTrigger,
        Button::RightTrigger2 => GamepadInputId::RightSecondaryTrigger,
        Button::Select => GamepadInputId::Select,
        Button::Start => GamepadInputId::Start,
        Button::Mode => GamepadInputId::Mode,
        Button::LeftThumb => GamepadInputId::LeftThumb,
        Button::RightThumb => GamepadInputId::RightThumb,
        Button::DPadUp => GamepadInputId::DPadUp,
        Button::DPadDown => GamepadInputId::DPadDown,
        Button::DPadLeft => GamepadInputId::DPadLeft,
        Button::DPadRight => GamepadInputId::DPadRight,
        Button::Unknown => return None,
    }))
}

pub fn gilrs_axis2input(axis: Axis, value: f32) -> Option<(InputId, InputState)> {
    match axis {
        Axis::LeftStickX => Some((
            InputId::Gamepad(if value < 0.0 {
                GamepadInputId::LeftStickLeft
            } else {
                GamepadInputId::LeftStickRight
            }),
            InputState::Analog(value.abs().clamp(0.0, 1.0)),
        )),
        Axis::LeftStickY => Some((
            InputId::Gamepad(if value < 0.0 {
                GamepadInputId::LeftStickDown
            } else {
                GamepadInputId::LeftStickUp
            }),
            InputState::Analog(value.abs().clamp(0.0, 1.0)),
        )),
        Axis::RightStickX => Some((
            InputId::Gamepad(if value < 0.0 {
                GamepadInputId::RightStickLeft
            } else {
                GamepadInputId::RightStickRight
            }),
            InputState::Analog(value.abs().clamp(0.0, 1.0)),
        )),
        Axis::RightStickY => Some((
            InputId::Gamepad(if value < 0.0 {
                GamepadInputId::RightStickDown
            } else {
                GamepadInputId::RightStickUp
            }),
            InputState::Analog(value.abs().clamp(0.0, 1.0)),
        )),
        // Needs investigation what this actually means
        Axis::LeftZ => todo!(),
        Axis::RightZ => todo!(),
        Axis::DPadX => Some((
            InputId::Gamepad(if value < 0.0 {
                GamepadInputId::DPadLeft
            } else {
                GamepadInputId::DPadRight
            }),
            InputState::Analog(value.abs().clamp(0.0, 1.0)),
        )),
        Axis::DPadY => Some((
            InputId::Gamepad(if value < 0.0 {
                GamepadInputId::DPadUp
            } else {
                GamepadInputId::DPadDown
            }),
            InputState::Analog(value.abs().clamp(0.0, 1.0)),
        )),
        Axis::Unknown => None,
    }
}

fn produce_id_for_gilrs_gamepad(
    non_stable_controller_identification: &mut HashMap<GamepadId, Uuid>,
    gilrs_gamepad_id: GamepadId,
    gilrs_gamepad: Gamepad<'_>,
) -> PhysicalInputDeviceId {
    let mut gamepad_id = Uuid::from_bytes(gilrs_gamepad.uuid());
    if gamepad_id == Uuid::nil() {
        gamepad_id = *non_stable_controller_identification
            .entry(gilrs_gamepad_id)
            .or_insert_with(|| {
                tracing::warn!(
                    "Gamepad {} is not giving us an ID, assigning it a arbitary one",
                    gamepad_id
                );

                Uuid::new_v4()
            });
    }

    PhysicalInputDeviceId::new(gamepad_id.try_into().unwrap())
}
