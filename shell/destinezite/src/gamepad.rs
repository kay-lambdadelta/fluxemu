use std::{collections::HashMap, time::Duration};

use fluxemu_frontend::{Frontend, FrontendPlatform};
use fluxemu_input::{GamepadInputId, InputId, InputState, physical::PhysicalInputDeviceId};
use gilrs::{Axis, Button, Event, GamepadId, Gilrs, GilrsBuilder};
use uuid::{NonNilUuid, Uuid};

#[inline]
fn convert_gilrs2input(button: Button) -> Option<InputId> {
    Some(InputId::Gamepad(match button {
        Button::South => GamepadInputId::FPadDown,
        Button::East => GamepadInputId::FPadRight,
        Button::North => GamepadInputId::FPadUp,
        Button::West => GamepadInputId::FPadLeft,
        Button::C => return None,
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

#[inline]
fn convert_gilrs2axis(axis: Axis, value: f32) -> Option<(InputId, InputState)> {
    match axis {
        Axis::LeftStickX => Some((
            InputId::Gamepad(if value < 0.0 {
                GamepadInputId::LeftStickLeft
            } else {
                GamepadInputId::LeftStickRight
            }),
            InputState(value.abs().clamp(0.0, 1.0)),
        )),
        Axis::LeftStickY => Some((
            InputId::Gamepad(if value < 0.0 {
                GamepadInputId::LeftStickDown
            } else {
                GamepadInputId::LeftStickUp
            }),
            InputState(value.abs().clamp(0.0, 1.0)),
        )),
        Axis::RightStickX => Some((
            InputId::Gamepad(if value < 0.0 {
                GamepadInputId::RightStickLeft
            } else {
                GamepadInputId::RightStickRight
            }),
            InputState(value.abs().clamp(0.0, 1.0)),
        )),
        Axis::RightStickY => Some((
            InputId::Gamepad(if value < 0.0 {
                GamepadInputId::RightStickDown
            } else {
                GamepadInputId::RightStickUp
            }),
            InputState(value.abs().clamp(0.0, 1.0)),
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
            InputState(value.abs().clamp(0.0, 1.0)),
        )),
        Axis::DPadY => Some((
            InputId::Gamepad(if value < 0.0 {
                GamepadInputId::DPadUp
            } else {
                GamepadInputId::DPadDown
            }),
            InputState(value.abs().clamp(0.0, 1.0)),
        )),
        Axis::Unknown => None,
    }
}

#[inline]
fn calculate_gamepad_id(gamepad: gilrs::Gamepad<'_>) -> (PhysicalInputDeviceId, bool) {
    if let Some(uuid) = NonNilUuid::new(Uuid::from_bytes(gamepad.uuid())) {
        (PhysicalInputDeviceId::new(uuid), true)
    } else {
        tracing::warn!(
            "Gamepad {} is not giving us an ID, assigning it a arbitary one",
            gamepad.name()
        );

        (PhysicalInputDeviceId(Uuid::new_v4()), false)
    }
}

#[derive(Debug)]
struct IdMappings(HashMap<GamepadId, PhysicalInputDeviceId>);

#[derive(Debug)]
pub struct GamepadContext {
    id_mappings: IdMappings,
    gilrs: Gilrs,
}

impl GamepadContext {
    #[allow(clippy::result_large_err)]
    pub fn new<P: FrontendPlatform>(frontend: &mut Frontend<P>) -> Result<Self, gilrs::Error> {
        let gilrs = GilrsBuilder::new()
            .add_env_mappings(true)
            .add_included_mappings(true)
            .set_update_state(true)
            .build()?;

        let mut id_mappings = IdMappings(HashMap::new());

        // Register existing gamepads
        for (gamepad_id, gamepad) in gilrs.gamepads() {
            let (physical_id, is_stable) = calculate_gamepad_id(gamepad);
            id_mappings.0.insert(gamepad_id, physical_id);

            frontend.register_gamepad(physical_id, gamepad.name().to_string(), is_stable, true);
        }

        Ok(Self { id_mappings, gilrs })
    }

    #[must_use]
    pub fn poll_gamepad_events<P: FrontendPlatform>(
        &mut self,
        timeout: Option<Duration>,
    ) -> Option<impl FnOnce(&mut Frontend<P>)> {
        let Event { id, event, .. } = self.gilrs.next_event_blocking(timeout)?;

        Some(move |frontend: &mut Frontend<P>| {
            let gamepad = self.gilrs.gamepad(id);

            match event {
                gilrs::EventType::Connected => {
                    let (physical_id, is_stable) = calculate_gamepad_id(gamepad);

                    frontend.register_gamepad(
                        physical_id,
                        gamepad.name().to_string(),
                        is_stable,
                        true,
                    );
                }
                gilrs::EventType::Disconnected => {
                    let physical_id = self.id_mappings.0[&id];

                    frontend.unregister_gamepad(physical_id);
                }
                gilrs::EventType::ButtonChanged(button, value, _) => {
                    if let Some(button) = convert_gilrs2input(button) {
                        let physical_id = self.id_mappings.0[&id];

                        frontend.insert_input(physical_id, button, InputState(value));
                    } else {
                        tracing::warn!("Did not recognize button: {:?}", button);
                    }
                }
                gilrs::EventType::AxisChanged(axis, value, _) => {
                    if let Some((input_id, state)) = convert_gilrs2axis(axis, value) {
                        let physical_id = self.id_mappings.0[&id];

                        frontend.insert_input(physical_id, input_id, state);
                    } else {
                        tracing::warn!("Did not recognize axis: {:?}", axis);
                    }
                }
                _ => {}
            }
        })
    }
}
