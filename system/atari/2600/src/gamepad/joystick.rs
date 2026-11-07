use std::sync::Arc;

use fluxemu_input::{GamepadInputId, InputId, KeyboardInputId};
use fluxemu_runtime::{
    component::{Component, config::ComponentConfig},
    input::LogicalInputDevice,
    machine::builder::ComponentBuilder,
    memory::{Address, AddressSpaceId},
    platform::Platform,
};

#[derive(Debug)]
pub struct Joystick {
    player1: Arc<LogicalInputDevice>,
    player2: Arc<LogicalInputDevice>,
}

impl Component for Joystick {
    type Event = ();
    type Save = ();
    type Snapshot = ();

    fn memory_read(
        &mut self,
        _address: Address,
        _address_space: AddressSpaceId,
        _avoid_side_effects: bool,
        buffer: &mut [u8],
    ) -> Result<(), fluxemu_runtime::memory::MemoryError> {
        buffer[0] = !(direction_nibble(&self.player1) << 4 | direction_nibble(&self.player2));

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

#[inline]
fn direction_nibble(device: &LogicalInputDevice) -> u8 {
    let is_held = |input_id| {
        device
            .get_state(InputId::Gamepad(input_id))
            .as_digital(None)
    };

    (is_held(GamepadInputId::LeftStickUp) as u8)
        | (is_held(GamepadInputId::LeftStickDown) as u8) << 1
        | (is_held(GamepadInputId::LeftStickLeft) as u8) << 2
        | (is_held(GamepadInputId::LeftStickRight) as u8) << 3
}

impl<P: Platform> ComponentConfig<P> for JoystickConfig {
    type Component = Joystick;

    fn build_component(
        self,
        component_builder: ComponentBuilder<P, Self::Component>,
    ) -> Result<Self::Component, Box<dyn std::error::Error>> {
        let (component_builder, player1) =
            component_builder.input("player-1", PRESENT_INPUTS, DEFAULT_MAPPINGS);
        let (_, player2) = component_builder.input("player-2", PRESENT_INPUTS, DEFAULT_MAPPINGS);

        Ok(Joystick { player1, player2 })
    }
}

#[derive(Debug)]
pub struct JoystickConfig;

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
