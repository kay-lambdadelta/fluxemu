use std::{ops::RangeInclusive, sync::Arc};

use fluxemu_input::{GamepadInputId, InputId, KeyboardInputId};
use fluxemu_range::ContiguousRange;
use fluxemu_runtime::{
    component::{Component, config::ComponentConfig},
    input::LogicalInputDevice,
    machine::builder::ComponentBuilder,
    memory::{Address, AddressSpaceId, MemoryError, MemoryMapCommand, Permissions},
    platform::Platform,
};

const CONTROLLER_0: Address = 0x4016;

const READ_ORDER: [InputId; 8] = [
    InputId::Gamepad(GamepadInputId::FPadDown),
    InputId::Gamepad(GamepadInputId::FPadRight),
    InputId::Gamepad(GamepadInputId::Select),
    InputId::Gamepad(GamepadInputId::Start),
    InputId::Gamepad(GamepadInputId::DPadUp),
    InputId::Gamepad(GamepadInputId::DPadDown),
    InputId::Gamepad(GamepadInputId::DPadLeft),
    InputId::Gamepad(GamepadInputId::DPadRight),
];

#[derive(Debug, Default)]
struct ControllerState {
    current_reads: [u8; 2],
    strobe: bool,
}

#[derive(Debug)]
pub struct StandardNesControllers {
    logical_input_devices: heapless::Vec<Arc<LogicalInputDevice>, 2>,
    state: ControllerState,
}

impl Component for StandardNesControllers {
    type Event = ();

    fn memory_read(
        &mut self,
        address: Address,
        _address_space: AddressSpaceId,
        avoid_side_effects: bool,
        buffer: &mut [u8],
    ) -> Result<(), MemoryError> {
        let controller_index = address - CONTROLLER_0;
        let current_read = self.state.current_reads[controller_index];
        let input_device = &self.logical_input_devices[controller_index];

        let key_value = if (0..READ_ORDER.len() as u8).contains(&current_read) {
            input_device
                .get_state(READ_ORDER[current_read as usize])
                .as_digital(None)
        } else {
            true
        };
        buffer[0] = (buffer[0] & 0b1111_1110) | key_value as u8;

        if !avoid_side_effects && !self.state.strobe && current_read < READ_ORDER.len() as u8 {
            self.state.current_reads[controller_index] += 1;
        }

        Ok(())
    }

    fn memory_write(
        &mut self,
        _address: Address,
        _address_space: AddressSpaceId,
        buffer: &[u8],
    ) -> Result<(), MemoryError> {
        self.state.strobe = buffer[0] & 0b0000_0001 != 0;

        if self.state.strobe {
            self.state.current_reads.fill(0);
        }

        Ok(())
    }
}

impl<P: Platform> ComponentConfig<P> for NesControllerConfig {
    type Component = StandardNesControllers;

    fn build_component(
        self,
        mut component_builder: ComponentBuilder<P, Self::Component>,
    ) -> Result<Self::Component, Box<dyn core::error::Error>> {
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

        let mut logical_input_devices = heapless::Vec::default();

        for controller_index in 0..2 {
            let (cb, input_device) = component_builder.input(
                get_controller_name(controller_index),
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

            logical_input_devices.push(input_device).unwrap();

            component_builder = cb;
        }

        // Grab both controllers
        let my_path = component_builder.path().clone();
        component_builder.map_memory(
            self.cpu_address_space,
            MemoryMapCommand::with_component(
                my_path,
                [
                    (
                        RangeInclusive::from_start_and_length(CONTROLLER_0, 2),
                        Permissions::READ,
                    ),
                    (
                        RangeInclusive::from_single(CONTROLLER_0),
                        Permissions::WRITE,
                    ),
                ],
            ),
        );

        Ok(StandardNesControllers {
            logical_input_devices,
            state: ControllerState::default(),
        })
    }
}

#[derive(Debug)]
pub struct NesControllerConfig {
    pub cpu_address_space: AddressSpaceId,
}

fn get_controller_name(index: u8) -> String {
    format!("standard-controller-{}", index)
}
