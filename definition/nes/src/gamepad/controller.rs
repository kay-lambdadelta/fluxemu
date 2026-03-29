use std::sync::{Arc, Mutex};

use fluxemu_input::{GamepadInputId, InputId, KeyboardInputId};
use fluxemu_runtime::{
    component::{Component, config::ComponentConfig},
    input::LogicalInputDevice,
    machine::builder::ComponentBuilder,
    memory::{Address, AddressSpaceId, MemoryError},
    platform::Platform,
};

const CONTROLLER_0: Address = 0x4016;

const READ_ORDER: [InputId; 8] = [
    InputId::Gamepad(GamepadInputId::FPadRight),
    InputId::Gamepad(GamepadInputId::FPadDown),
    InputId::Gamepad(GamepadInputId::Select),
    InputId::Gamepad(GamepadInputId::Start),
    InputId::Gamepad(GamepadInputId::DPadUp),
    InputId::Gamepad(GamepadInputId::DPadDown),
    InputId::Gamepad(GamepadInputId::DPadLeft),
    InputId::Gamepad(GamepadInputId::DPadRight),
];

#[derive(Debug, Default)]
struct ControllerState {
    current_read: u8,
    strobe: bool,
}

#[derive(Debug)]
pub struct NesController {
    input_state: Arc<LogicalInputDevice>,
    state: Mutex<ControllerState>,
}

impl Component for NesController {
    fn load_snapshot(
        &mut self,
        _version: fluxemu_runtime::component::ComponentVersion,
        _reader: &mut dyn std::io::Read,
    ) -> Result<(), Box<dyn std::error::Error>> {
        todo!()
    }

    fn store_snapshot(
        &self,
        _writer: &mut dyn std::io::Write,
    ) -> Result<(), Box<dyn std::error::Error>> {
        todo!()
    }

    fn memory_read(
        &self,
        _address: Address,
        _address_space: AddressSpaceId,
        avoid_side_effects: bool,
        buffer: &mut [u8],
    ) -> Result<(), MemoryError> {
        let mut state_guard = self.state.lock().unwrap();

        let key_value = if (0..READ_ORDER.len() as u8).contains(&state_guard.current_read) {
            self.input_state
                .get_state(READ_ORDER[state_guard.current_read as usize])
                .as_digital(None)
        } else {
            true
        };
        buffer[0] = (buffer[0] & 0b1111_1110) | key_value as u8;

        if !avoid_side_effects
            && !state_guard.strobe
            && state_guard.current_read < READ_ORDER.len() as u8
        {
            state_guard.current_read += 1;
        }

        Ok(())
    }

    fn memory_write(
        &mut self,
        _address: Address,
        _address_space: AddressSpaceId,
        buffer: &[u8],
    ) -> Result<(), MemoryError> {
        let mut state = self.state.lock().unwrap();
        state.strobe = buffer[0] & 0b0000_0001 != 0;

        if state.strobe {
            state.current_read = 0;
        }

        Ok(())
    }
}

impl<P: Platform> ComponentConfig<P> for NesControllerConfig {
    type Component = NesController;

    fn build_component(
        self,
        component_builder: ComponentBuilder<'_, '_, P, Self::Component>,
    ) -> Result<Self::Component, Box<dyn std::error::Error>> {
        let present_inputs = [
            InputId::Gamepad(GamepadInputId::DPadUp),
            InputId::Gamepad(GamepadInputId::DPadDown),
            InputId::Gamepad(GamepadInputId::DPadLeft),
            InputId::Gamepad(GamepadInputId::DPadRight),
            InputId::Gamepad(GamepadInputId::FPadDown),
            InputId::Gamepad(GamepadInputId::FPadRight),
            InputId::Gamepad(GamepadInputId::Start),
            InputId::Gamepad(GamepadInputId::Select),
        ];

        let controller_name = get_controller_name(self.controller_index);

        let (component_builder, input_state) = component_builder.input(
            controller_name,
            present_inputs,
            present_inputs
                .into_iter()
                .map(|input| (input, input))
                .chain([
                    (
                        InputId::Keyboard(KeyboardInputId::ArrowDown),
                        InputId::Gamepad(GamepadInputId::DPadDown),
                    ),
                    (
                        InputId::Keyboard(KeyboardInputId::ArrowUp),
                        InputId::Gamepad(GamepadInputId::DPadUp),
                    ),
                    (
                        InputId::Keyboard(KeyboardInputId::ArrowLeft),
                        InputId::Gamepad(GamepadInputId::DPadLeft),
                    ),
                    (
                        InputId::Keyboard(KeyboardInputId::ArrowRight),
                        InputId::Gamepad(GamepadInputId::DPadRight),
                    ),
                    (
                        InputId::Keyboard(KeyboardInputId::KeyZ),
                        InputId::Gamepad(GamepadInputId::FPadDown),
                    ),
                    (
                        InputId::Keyboard(KeyboardInputId::KeyX),
                        InputId::Gamepad(GamepadInputId::FPadRight),
                    ),
                    (
                        InputId::Keyboard(KeyboardInputId::Enter),
                        InputId::Gamepad(GamepadInputId::Start),
                    ),
                    (
                        InputId::Keyboard(KeyboardInputId::ShiftRight),
                        InputId::Gamepad(GamepadInputId::Select),
                    ),
                ]),
        );

        let register_location = CONTROLLER_0 + self.controller_index as usize;
        let component_builder = component_builder.memory_map_component_read(
            self.cpu_address_space,
            register_location..=register_location,
        );

        // FIXME: The two controllers need to share this state

        component_builder
            .memory_map_component_write(self.cpu_address_space, CONTROLLER_0..=CONTROLLER_0);

        Ok(NesController {
            input_state,
            state: Mutex::default(),
        })
    }
}

#[derive(Debug)]
pub struct NesControllerConfig {
    pub cpu_address_space: AddressSpaceId,
    pub controller_index: u8,
}

fn get_controller_name(index: u8) -> String {
    format!("standard-controller-{}", index + 1)
}
