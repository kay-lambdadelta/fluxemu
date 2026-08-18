use std::{collections::HashMap, io::Write, time::Duration};

use digest_io::IoWrapper;
use fluxemu_frontend::{Frontend, FrontendPlatform};
use fluxemu_input::{GamepadInputId, InputId, InputState, physical::PhysicalInputDeviceId};
use gilrs::{Axis, Button, Event, GamepadId, Gilrs, GilrsBuilder};
use sha2::{Digest, Sha256};
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
            InputState::new(value.abs()),
        )),
        Axis::LeftStickY => Some((
            InputId::Gamepad(if value < 0.0 {
                GamepadInputId::LeftStickDown
            } else {
                GamepadInputId::LeftStickUp
            }),
            InputState::new(value.abs()),
        )),
        Axis::RightStickX => Some((
            InputId::Gamepad(if value < 0.0 {
                GamepadInputId::RightStickLeft
            } else {
                GamepadInputId::RightStickRight
            }),
            InputState::new(value.abs()),
        )),
        Axis::RightStickY => Some((
            InputId::Gamepad(if value < 0.0 {
                GamepadInputId::RightStickDown
            } else {
                GamepadInputId::RightStickUp
            }),
            InputState::new(value.abs()),
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
            InputState::new(value.abs()),
        )),
        Axis::DPadY => Some((
            InputId::Gamepad(if value < 0.0 {
                GamepadInputId::DPadUp
            } else {
                GamepadInputId::DPadDown
            }),
            InputState::new(value.abs()),
        )),
        Axis::Unknown => None,
    }
}

#[inline]
fn calculate_gamepad_id(gamepad: gilrs::Gamepad<'_>) -> PhysicalInputDeviceId {
    if let Some(uuid) = NonNilUuid::new(Uuid::from_bytes(gamepad.uuid())) {
        PhysicalInputDeviceId::new(uuid)
    } else {
        tracing::warn!(
            "Gamepad {} is not giving us an ID, assigning it one",
            gamepad.name()
        );

        let mut hasher = IoWrapper(Sha256::default());
        hasher.write_all(gamepad.name().as_bytes()).unwrap();
        let hash: [u8; 32] = hasher.0.finalize().into();

        let uuid = Uuid::from_slice(&hash[..16]).unwrap();

        PhysicalInputDeviceId(uuid)
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
            let physical_id = calculate_gamepad_id(gamepad);
            id_mappings.0.insert(gamepad_id, physical_id);

            frontend.register_gamepad(physical_id, gamepad.name().to_string(), true);
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
                    let physical_id = calculate_gamepad_id(gamepad);
                    self.id_mappings.0.insert(id, physical_id);

                    frontend.register_gamepad(physical_id, gamepad.name().to_string(), true);
                }
                gilrs::EventType::Disconnected => {
                    let physical_id = self.id_mappings.0[&id];

                    frontend.unregister_gamepad(physical_id);
                }
                gilrs::EventType::ButtonChanged(button, value, _) => {
                    if let Some(button) = convert_gilrs2input(button) {
                        let physical_id = self.id_mappings.0[&id];

                        frontend.insert_input(physical_id, button, InputState::new(value));
                    } else {
                        tracing::debug!("Did not recognize button: {:?}", button);
                    }
                }
                gilrs::EventType::AxisChanged(axis, value, _) => {
                    if let Some((input_id, state)) = convert_gilrs2axis(axis, value) {
                        let physical_id = self.id_mappings.0[&id];

                        frontend.insert_input(physical_id, input_id, state);
                    } else {
                        tracing::debug!("Did not recognize axis: {:?}", axis);
                    }
                }
                _ => {}
            }
        })
    }
}
