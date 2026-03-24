use fluxemu_input::{GamepadInputId, InputId, InputState};
use gilrs::{Axis, Button};

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
