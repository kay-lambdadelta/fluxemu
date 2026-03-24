use std::{collections::HashMap, sync::Arc};

use bitvec::{prelude::Lsb0, view::BitView};
use fluxemu_input::{GamepadInputId, InputId, InputState, KeyboardInputId};
use fluxemu_runtime::{
    component::{Component, ComponentConfig},
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
    fn memory_read(
        &self,
        _address: Address,
        _address_space: AddressSpaceId,
        _avoid_side_effects: bool,
        buffer: &mut [u8],
    ) -> Result<(), fluxemu_runtime::memory::MemoryError> {
        // If this function is called we are mapped to swcha

        let value_bits = buffer.view_bits_mut::<Lsb0>();
        let (player1, player2) = value_bits.split_at_mut(4);

        player1.set(
            0,
            self.player1
                .get_state(InputId::Gamepad(GamepadInputId::LeftStickUp))
                .as_digital(None),
        );
        player1.set(
            1,
            self.player1
                .get_state(InputId::Gamepad(GamepadInputId::LeftStickDown))
                .as_digital(None),
        );
        player1.set(
            2,
            self.player1
                .get_state(InputId::Gamepad(GamepadInputId::LeftStickLeft))
                .as_digital(None),
        );
        player1.set(
            3,
            self.player1
                .get_state(InputId::Gamepad(GamepadInputId::LeftStickRight))
                .as_digital(None),
        );

        player2.set(
            0,
            self.player2
                .get_state(InputId::Gamepad(GamepadInputId::LeftStickUp))
                .as_digital(None),
        );
        player2.set(
            1,
            self.player2
                .get_state(InputId::Gamepad(GamepadInputId::LeftStickDown))
                .as_digital(None),
        );
        player2.set(
            2,
            self.player2
                .get_state(InputId::Gamepad(GamepadInputId::LeftStickLeft))
                .as_digital(None),
        );
        player2.set(
            3,
            self.player2
                .get_state(InputId::Gamepad(GamepadInputId::LeftStickRight))
                .as_digital(None),
        );

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
