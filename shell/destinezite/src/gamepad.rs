use std::{collections::HashMap, time::Duration};

use fluxemu_input::{
    GamepadInputId, InputId, InputState,
    physical::{PhysicalInputDeviceId, PhysicalInputDeviceInstanceId, PhysicalInputDeviceKind},
};
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
fn split_axis(
    negative: GamepadInputId,
    positive: GamepadInputId,
    value: f32,
) -> [(InputId, InputState); 2] {
    [
        (InputId::Gamepad(negative), InputState::new(-value)),
        (InputId::Gamepad(positive), InputState::new(value)),
    ]
}

#[inline]
fn convert_gilrs2axis(axis: Axis, value: f32) -> Option<[(InputId, InputState); 2]> {
    match axis {
        Axis::LeftStickX => Some(split_axis(
            GamepadInputId::LeftStickLeft,
            GamepadInputId::LeftStickRight,
            value,
        )),
        Axis::LeftStickY => Some(split_axis(
            GamepadInputId::LeftStickDown,
            GamepadInputId::LeftStickUp,
            value,
        )),
        Axis::RightStickX => Some(split_axis(
            GamepadInputId::RightStickLeft,
            GamepadInputId::RightStickRight,
            value,
        )),
        Axis::RightStickY => Some(split_axis(
            GamepadInputId::RightStickDown,
            GamepadInputId::RightStickUp,
            value,
        )),
        // Needs investigation what this actually means
        Axis::LeftZ | Axis::RightZ => None,
        Axis::DPadX => Some(split_axis(
            GamepadInputId::DPadLeft,
            GamepadInputId::DPadRight,
            value,
        )),
        Axis::DPadY => Some(split_axis(
            GamepadInputId::DPadDown,
            GamepadInputId::DPadUp,
            value,
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

        let hash = blake3::hash(gamepad.name().as_bytes());

        let uuid = Uuid::from_slice(&hash.as_bytes()[..16]).unwrap();

        PhysicalInputDeviceId(uuid)
    }
}

pub trait GamepadSink {
    fn register_gamepad(&mut self, id: PhysicalInputDeviceInstanceId, name: String);
    fn unregister_gamepad(&mut self, id: PhysicalInputDeviceInstanceId);
    fn insert_input(
        &mut self,
        id: PhysicalInputDeviceInstanceId,
        input_id: InputId,
        state: InputState,
    );
}

#[cfg(feature = "egui")]
impl<P: fluxemu_frontend::Platform> GamepadSink for fluxemu_frontend_egui::Frontend<P> {
    fn register_gamepad(&mut self, id: PhysicalInputDeviceInstanceId, name: String) {
        self.register_gamepad(id, name, PhysicalInputDeviceKind::Gamepad, true);
    }

    fn unregister_gamepad(&mut self, id: PhysicalInputDeviceInstanceId) {
        self.unregister_gamepad(id);
    }

    fn insert_input(
        &mut self,
        id: PhysicalInputDeviceInstanceId,
        input_id: InputId,
        state: InputState,
    ) {
        self.insert_input(id, input_id, state);
    }
}

#[cfg(feature = "ratatui")]
impl GamepadSink for fluxemu_frontend_ratatui::ExternalInputSender {
    fn register_gamepad(&mut self, id: PhysicalInputDeviceInstanceId, name: String) {
        fluxemu_frontend_ratatui::ExternalInputSender::register_gamepad(self, id, name);
    }

    fn unregister_gamepad(&mut self, id: PhysicalInputDeviceInstanceId) {
        fluxemu_frontend_ratatui::ExternalInputSender::unregister_gamepad(self, id);
    }

    fn insert_input(
        &mut self,
        id: PhysicalInputDeviceInstanceId,
        input_id: InputId,
        state: InputState,
    ) {
        fluxemu_frontend_ratatui::ExternalInputSender::insert_input(self, id, input_id, state);
    }
}

#[derive(Debug)]
struct IdMappings(HashMap<GamepadId, PhysicalInputDeviceInstanceId>);

impl IdMappings {
    fn allocate(
        &mut self,
        gamepad_id: GamepadId,
        device: PhysicalInputDeviceId,
    ) -> Option<PhysicalInputDeviceInstanceId> {
        if self.0.contains_key(&gamepad_id) {
            return None;
        }

        let instance = (0..=u8::MAX)
            .map(|slot| PhysicalInputDeviceInstanceId::new(device, slot))
            .find(|instance| !self.0.values().any(|used| used == instance))
            .expect("Ran out of slots for identical gamepads");

        self.0.insert(gamepad_id, instance);

        Some(instance)
    }
}

#[derive(Debug)]
pub struct GamepadContext {
    id_mappings: IdMappings,
    gilrs: Gilrs,
}

impl GamepadContext {
    #[allow(clippy::result_large_err)]
    pub fn new<S: GamepadSink>(frontend: &mut S) -> Result<Self, gilrs::Error> {
        let gilrs = GilrsBuilder::new()
            .add_env_mappings(true)
            .add_included_mappings(true)
            .set_update_state(true)
            .build()?;

        let mut id_mappings = IdMappings(HashMap::new());

        // Register existing gamepads
        for (gamepad_id, gamepad) in gilrs.gamepads() {
            let device = calculate_gamepad_id(gamepad);

            if let Some(instance) = id_mappings.allocate(gamepad_id, device) {
                frontend.register_gamepad(instance, gamepad.name().to_string());
            }
        }

        Ok(Self { id_mappings, gilrs })
    }

    #[must_use]
    pub fn poll_gamepad_events<S: GamepadSink>(
        &mut self,
        timeout: Option<Duration>,
    ) -> Option<impl FnOnce(&mut S)> {
        let Event { id, event, .. } = self.gilrs.next_event_blocking(timeout)?;

        Some(move |frontend: &mut S| {
            let gamepad = self.gilrs.gamepad(id);

            match event {
                gilrs::EventType::Connected => {
                    let device = calculate_gamepad_id(gamepad);

                    if let Some(instance) = self.id_mappings.allocate(id, device) {
                        frontend.register_gamepad(instance, gamepad.name().to_string());
                    }
                }
                gilrs::EventType::Disconnected => {
                    if let Some(instance) = self.id_mappings.0.remove(&id) {
                        frontend.unregister_gamepad(instance);
                    }
                }
                gilrs::EventType::ButtonChanged(button, value, _) => {
                    let Some(&instance) = self.id_mappings.0.get(&id) else {
                        return;
                    };

                    if let Some(button) = convert_gilrs2input(button) {
                        frontend.insert_input(instance, button, InputState::new(value));
                    } else {
                        tracing::debug!("Did not recognize button: {:?}", button);
                    }
                }
                gilrs::EventType::AxisChanged(axis, value, _) => {
                    let Some(&instance) = self.id_mappings.0.get(&id) else {
                        return;
                    };

                    if let Some(inputs) = convert_gilrs2axis(axis, value) {
                        for (input_id, state) in inputs {
                            frontend.insert_input(instance, input_id, state);
                        }
                    } else {
                        tracing::debug!("Did not recognize axis: {:?}", axis);
                    }
                }
                _ => {}
            }
        })
    }
}
