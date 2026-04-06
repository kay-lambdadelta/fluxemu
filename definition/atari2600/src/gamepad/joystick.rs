use std::{collections::HashMap, sync::Arc};

use fluxemu_input::{GamepadInputId, InputId, InputState, KeyboardInputId};
use fluxemu_runtime::{
    component::{Component, config::ComponentConfig},
    input::LogicalInputDevice,
    machine::builder::ComponentBuilder,
    memory::{Address, AddressSpaceId},
    platform::Platform,
};

#[derive(Debug)]
pub struct Atari2600Joystick {
    player1: Arc<LogicalInputDevice>,
    player2: Arc<LogicalInputDevice>,
}

impl Component for Atari2600Joystick {
    type Event = ();

    fn memory_read(
        &self,
        _address: Address,
        _address_space: AddressSpaceId,
        _avoid_side_effects: bool,
        buffer: &mut [u8],
    ) -> Result<(), fluxemu_runtime::memory::MemoryError> {
        // Player 1
        let up = self
            .player1
            .get_state(InputId::Gamepad(GamepadInputId::LeftStickUp))
            .as_digital(None);
        let down = self
            .player1
            .get_state(InputId::Gamepad(GamepadInputId::LeftStickDown))
            .as_digital(None);
        let left = self
            .player1
            .get_state(InputId::Gamepad(GamepadInputId::LeftStickLeft))
            .as_digital(None);
        let right = self
            .player1
            .get_state(InputId::Gamepad(GamepadInputId::LeftStickRight))
            .as_digital(None);

        buffer[0] = (up as u8) | (down as u8) << 1 | (left as u8) << 2 | (right as u8) << 3;

        // Player 2
        let up = self
            .player2
            .get_state(InputId::Gamepad(GamepadInputId::LeftStickUp))
            .as_digital(None);
        let down = self
            .player2
            .get_state(InputId::Gamepad(GamepadInputId::LeftStickDown))
            .as_digital(None);
        let left = self
            .player2
            .get_state(InputId::Gamepad(GamepadInputId::LeftStickLeft))
            .as_digital(None);
        let right = self
            .player2
            .get_state(InputId::Gamepad(GamepadInputId::LeftStickRight))
            .as_digital(None);

        buffer[0] |= (up as u8) << 4 | (down as u8) << 5 | (left as u8) << 6 | (right as u8) << 7;

        Ok(())
    }

    fn memory_write(
        &mut self,
        _address: Address,
        _address_space: AddressSpaceId,
        _buffer: &[u8],
    ) -> Result<(), fluxemu_runtime::memory::MemoryError> {
        Ok(())
    }
}

impl<P: Platform> ComponentConfig<P> for Atari2600JoystickConfig {
    type Component = Atari2600Joystick;

    fn build_component(
        self,
        component_builder: ComponentBuilder<'_, '_, P, Self::Component>,
    ) -> Result<Self::Component, Box<dyn std::error::Error>> {
        let (component_builder, player1) =
            component_builder.input("player-1", PRESENT_INPUTS, DEFAULT_MAPPINGS);
        let (_, player2) = component_builder.input("player-2", PRESENT_INPUTS, DEFAULT_MAPPINGS);

        Ok(Atari2600Joystick { player1, player2 })
    }
}

#[derive(Debug)]
pub struct Atari2600JoystickConfig;

#[derive(Debug)]
pub struct JoystickSwchaCallback {
    gamepads: [HashMap<InputId, InputState>; 2],
}

const PRESENT_INPUTS: [InputId; 5] = [
    InputId::Gamepad(GamepadInputId::LeftStickUp),
    InputId::Gamepad(GamepadInputId::LeftStickDown),
    InputId::Gamepad(GamepadInputId::LeftStickLeft),
    InputId::Gamepad(GamepadInputId::LeftStickRight),
    InputId::Gamepad(GamepadInputId::FPadDown),
];

const DEFAULT_MAPPINGS: [(InputId, InputId); 10] = [
    (
        InputId::Gamepad(GamepadInputId::LeftStickUp),
        InputId::Gamepad(GamepadInputId::LeftStickUp),
    ),
    (
        InputId::Gamepad(GamepadInputId::LeftStickDown),
        InputId::Gamepad(GamepadInputId::LeftStickDown),
    ),
    (
        InputId::Gamepad(GamepadInputId::LeftStickLeft),
        InputId::Gamepad(GamepadInputId::LeftStickLeft),
    ),
    (
        InputId::Gamepad(GamepadInputId::LeftStickRight),
        InputId::Gamepad(GamepadInputId::LeftStickRight),
    ),
    (
        InputId::Gamepad(GamepadInputId::FPadDown),
        InputId::Gamepad(GamepadInputId::FPadDown),
    ),
    (
        InputId::Keyboard(KeyboardInputId::ArrowDown),
        InputId::Gamepad(GamepadInputId::LeftStickDown),
    ),
    (
        InputId::Keyboard(KeyboardInputId::ArrowUp),
        InputId::Gamepad(GamepadInputId::LeftStickUp),
    ),
    (
        InputId::Keyboard(KeyboardInputId::ArrowLeft),
        InputId::Gamepad(GamepadInputId::LeftStickLeft),
    ),
    (
        InputId::Keyboard(KeyboardInputId::ArrowRight),
        InputId::Gamepad(GamepadInputId::LeftStickRight),
    ),
    (
        InputId::Keyboard(KeyboardInputId::KeyZ),
        InputId::Gamepad(GamepadInputId::FPadDown),
    ),
];
