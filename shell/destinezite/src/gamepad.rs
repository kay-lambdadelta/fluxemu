use std::{collections::HashMap, time::Duration};

use fluxemu_input::{
    DirectionPart, GamepadInputId, InputId, InputShape, InputState,
    host::{HostInputDeviceId, HostInputDeviceInstanceId, HostInputDeviceKind},
};
use gilrs::{Axis, Button, Event, GamepadId, Gilrs, GilrsBuilder};
use nalgebra::Vector2;
use uuid::{NonNilUuid, Uuid};

#[inline]
fn convert_gilrs2input(button: Button) -> Option<GamepadInputId> {
    Some(match button {
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
        Button::DPadUp | Button::DPadDown | Button::DPadLeft | Button::DPadRight => {
            GamepadInputId::DPad(DirectionPart::All)
        }
        Button::Unknown => return None,
    })
}

#[inline]
fn convert_gilrs2axis(axis: Axis) -> Option<GamepadInputId> {
    Some(match axis {
        Axis::LeftStickX | Axis::LeftStickY => GamepadInputId::LeftStick(DirectionPart::All),
        Axis::RightStickX | Axis::RightStickY => GamepadInputId::RightStick(DirectionPart::All),
        Axis::DPadX | Axis::DPadY => GamepadInputId::DPad(DirectionPart::All),
        // Needs investigation what this actually means
        Axis::LeftZ | Axis::RightZ => return None,
        Axis::Unknown => return None,
    })
}

fn gamepad_value(gamepad: gilrs::Gamepad<'_>, input_id: GamepadInputId) -> InputState {
    let pressed = |button| if gamepad.is_pressed(button) { 1.0 } else { 0.0 };
    let stick = |x_axis, y_axis| {
        InputState::direction(Vector2::new(gamepad.value(x_axis), -gamepad.value(y_axis)))
    };

    match input_id {
        GamepadInputId::DPad(_) => InputState::direction(Vector2::new(
            pressed(Button::DPadRight) - pressed(Button::DPadLeft) + gamepad.value(Axis::DPadX),
            pressed(Button::DPadDown) - pressed(Button::DPadUp) - gamepad.value(Axis::DPadY),
        )),
        GamepadInputId::LeftStick(_) => stick(Axis::LeftStickX, Axis::LeftStickY),
        GamepadInputId::RightStick(_) => stick(Axis::RightStickX, Axis::RightStickY),
        _ => unreachable!("{input_id:?} is not a direction"),
    }
}

#[inline]
fn calculate_gamepad_id(gamepad: gilrs::Gamepad<'_>) -> HostInputDeviceId {
    if let Some(uuid) = NonNilUuid::new(Uuid::from_bytes(gamepad.uuid())) {
        HostInputDeviceId::new(uuid)
    } else {
        tracing::warn!(
            "Gamepad {} is not giving us an ID, assigning it one",
            gamepad.name()
        );

        let hash = blake3::hash(gamepad.name().as_bytes());
        let uuid = Uuid::from_slice(&hash.as_bytes()[..16]).unwrap();

        HostInputDeviceId(uuid)
    }
}

pub trait GamepadSink {
    fn register_gamepad(&mut self, id: HostInputDeviceInstanceId, name: String);
    fn unregister_gamepad(&mut self, id: HostInputDeviceInstanceId);
    fn insert_input(&mut self, id: HostInputDeviceInstanceId, input_id: InputId, state: InputState);
}

#[cfg(feature = "egui")]
impl<P: fluxemu_frontend::Platform> GamepadSink for fluxemu_frontend_egui::Frontend<P> {
    fn register_gamepad(&mut self, id: HostInputDeviceInstanceId, name: String) {
        self.register_gamepad(id, name, HostInputDeviceKind::Gamepad, true);
    }

    fn unregister_gamepad(&mut self, id: HostInputDeviceInstanceId) {
        self.unregister_gamepad(id);
    }

    fn insert_input(
        &mut self,
        id: HostInputDeviceInstanceId,
        input_id: InputId,
        state: InputState,
    ) {
        self.insert_input(id, input_id, state);
    }
}

#[cfg(feature = "egui")]
#[derive(Debug)]
pub enum InputMessage {
    Register {
        id: HostInputDeviceInstanceId,
        name: String,
        kind: HostInputDeviceKind,
        rely_on_frontend_input_handling: bool,
    },
    Unregister {
        id: HostInputDeviceInstanceId,
    },
    Insert {
        id: HostInputDeviceInstanceId,
        input_id: InputId,
        state: InputState,
    },
}

#[cfg(feature = "egui")]
impl InputMessage {
    pub fn apply<P: fluxemu_frontend::Platform>(
        self,
        frontend: &mut fluxemu_frontend_egui::Frontend<P>,
    ) {
        match self {
            InputMessage::Register {
                id,
                name,
                kind,
                rely_on_frontend_input_handling,
            } => frontend.register_gamepad(id, name, kind, rely_on_frontend_input_handling),
            InputMessage::Unregister { id } => frontend.unregister_gamepad(id),
            InputMessage::Insert {
                id,
                input_id,
                state,
            } => frontend.insert_input(id, input_id, state),
        }
    }
}

#[cfg(feature = "egui")]
pub struct InputMessageSink<F: FnMut(InputMessage)>(pub F);

#[cfg(feature = "egui")]
impl<F: FnMut(InputMessage)> GamepadSink for InputMessageSink<F> {
    fn register_gamepad(&mut self, id: HostInputDeviceInstanceId, name: String) {
        (self.0)(InputMessage::Register {
            id,
            name,
            kind: HostInputDeviceKind::Gamepad,
            rely_on_frontend_input_handling: true,
        });
    }

    fn unregister_gamepad(&mut self, id: HostInputDeviceInstanceId) {
        (self.0)(InputMessage::Unregister { id });
    }

    fn insert_input(
        &mut self,
        id: HostInputDeviceInstanceId,
        input_id: InputId,
        state: InputState,
    ) {
        (self.0)(InputMessage::Insert {
            id,
            input_id,
            state,
        });
    }
}

#[cfg(feature = "ratatui")]
impl GamepadSink for fluxemu_frontend_ratatui::ExternalInputSender {
    fn register_gamepad(&mut self, id: HostInputDeviceInstanceId, name: String) {
        fluxemu_frontend_ratatui::ExternalInputSender::register_gamepad(self, id, name);
    }

    fn unregister_gamepad(&mut self, id: HostInputDeviceInstanceId) {
        fluxemu_frontend_ratatui::ExternalInputSender::unregister_gamepad(self, id);
    }

    fn insert_input(
        &mut self,
        id: HostInputDeviceInstanceId,
        input_id: InputId,
        state: InputState,
    ) {
        fluxemu_frontend_ratatui::ExternalInputSender::insert_input(self, id, input_id, state);
    }
}

#[derive(Debug)]
struct IdMappings(HashMap<GamepadId, HostInputDeviceInstanceId>);

impl IdMappings {
    fn allocate(
        &mut self,
        gamepad_id: GamepadId,
        device: HostInputDeviceId,
    ) -> Option<HostInputDeviceInstanceId> {
        if self.0.contains_key(&gamepad_id) {
            return None;
        }

        let instance = (0..=u8::MAX)
            .map(|slot| HostInputDeviceInstanceId::new(device, slot))
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

                    if let Some(input_id) = convert_gilrs2input(button) {
                        let value = match input_id.shape() {
                            InputShape::Button => InputState::button(value),
                            InputShape::Direction => gamepad_value(gamepad, input_id),
                        };

                        frontend.insert_input(instance, InputId::Gamepad(input_id), value);
                    } else {
                        tracing::debug!("Did not recognize button: {:?}", button);
                    }
                }
                gilrs::EventType::AxisChanged(axis, _, _) => {
                    let Some(&instance) = self.id_mappings.0.get(&id) else {
                        return;
                    };

                    if let Some(input_id) = convert_gilrs2axis(axis) {
                        frontend.insert_input(
                            instance,
                            InputId::Gamepad(input_id),
                            gamepad_value(gamepad, input_id),
                        );
                    } else {
                        tracing::debug!("Did not recognize axis: {:?}", axis);
                    }
                }
                _ => {}
            }
        })
    }
}
