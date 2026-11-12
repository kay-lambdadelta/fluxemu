use std::sync::Arc;

use fluxemu_definition_mos6532::Register;
use fluxemu_input::{GamepadInputId, InputId};
use fluxemu_platform::Platform;
use fluxemu_runtime::{
    component::{Component, config::ComponentConfig},
    input::LogicalInputDevice,
    machine::builder::ComponentBuilder,
    memory::{Address, AddressSpaceId},
};
use keyboard_types::Code;

const IDLE_CONSOLE_SWITCHES: u8 = 0b0000_1000;

#[derive(Debug)]
pub struct Joystick {
    players: [Arc<LogicalInputDevice>; 2],
    riot_registers_address: Address,
}

impl Joystick {
    pub fn fire_pressed(&self, player: usize) -> bool {
        is_held(&self.players[player], GamepadInputId::FPadDown)
    }

    fn console_switches(&self) -> u8 {
        let any_player_holds =
            |input_id| self.players.iter().any(|player| is_held(player, input_id));

        let reset = any_player_holds(GamepadInputId::Start);
        let select = any_player_holds(GamepadInputId::Select);

        IDLE_CONSOLE_SWITCHES | !reset as u8 | (!select as u8) << 1
    }
}

impl Component for Joystick {
    type Event = ();
    type Save = ();
    type Snapshot = ();

    fn memory_read(
        &mut self,
        address: Address,
        _address_space: AddressSpaceId,
        _avoid_side_effects: bool,
        buffer: &mut [u8],
    ) -> Result<(), fluxemu_runtime::memory::MemoryError> {
        buffer[0] = match Register::from_repr(address - self.riot_registers_address) {
            Some(Register::Swcha) => {
                !(direction_nibble(&self.players[0]) << 4 | direction_nibble(&self.players[1]))
            }
            Some(Register::Swchb) => self.console_switches(),
            register => unreachable!("{:?}", register),
        };

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
fn is_held(device: &LogicalInputDevice, input_id: GamepadInputId) -> bool {
    device
        .get_state(InputId::Gamepad(input_id))
        .as_digital(None)
}

#[inline]
fn direction_nibble(device: &LogicalInputDevice) -> u8 {
    (is_held(device, GamepadInputId::LeftStickUp) as u8)
        | (is_held(device, GamepadInputId::LeftStickDown) as u8) << 1
        | (is_held(device, GamepadInputId::LeftStickLeft) as u8) << 2
        | (is_held(device, GamepadInputId::LeftStickRight) as u8) << 3
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

        Ok(Joystick {
            players: [player1, player2],
            riot_registers_address: self.riot_registers_address,
        })
    }
}

#[derive(Debug)]
pub struct JoystickConfig {
    pub riot_registers_address: Address,
}

const PRESENT_INPUTS: [InputId; 7] = [
    InputId::Gamepad(GamepadInputId::LeftStickUp),
    InputId::Gamepad(GamepadInputId::LeftStickDown),
    InputId::Gamepad(GamepadInputId::LeftStickLeft),
    InputId::Gamepad(GamepadInputId::LeftStickRight),
    InputId::Gamepad(GamepadInputId::FPadDown),
    InputId::Gamepad(GamepadInputId::Start),
    InputId::Gamepad(GamepadInputId::Select),
];

const DEFAULT_MAPPINGS: [(InputId, InputId); 18] = [
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
        InputId::Gamepad(GamepadInputId::DPadUp),
        InputId::Gamepad(GamepadInputId::LeftStickUp),
    ),
    (
        InputId::Gamepad(GamepadInputId::DPadDown),
        InputId::Gamepad(GamepadInputId::LeftStickDown),
    ),
    (
        InputId::Gamepad(GamepadInputId::DPadLeft),
        InputId::Gamepad(GamepadInputId::LeftStickLeft),
    ),
    (
        InputId::Gamepad(GamepadInputId::DPadRight),
        InputId::Gamepad(GamepadInputId::LeftStickRight),
    ),
    (
        InputId::Gamepad(GamepadInputId::FPadDown),
        InputId::Gamepad(GamepadInputId::FPadDown),
    ),
    (
        InputId::Gamepad(GamepadInputId::Start),
        InputId::Gamepad(GamepadInputId::Start),
    ),
    (
        InputId::Gamepad(GamepadInputId::Select),
        InputId::Gamepad(GamepadInputId::Select),
    ),
    (
        InputId::Keyboard(Code::ArrowDown),
        InputId::Gamepad(GamepadInputId::LeftStickDown),
    ),
    (
        InputId::Keyboard(Code::ArrowUp),
        InputId::Gamepad(GamepadInputId::LeftStickUp),
    ),
    (
        InputId::Keyboard(Code::ArrowLeft),
        InputId::Gamepad(GamepadInputId::LeftStickLeft),
    ),
    (
        InputId::Keyboard(Code::ArrowRight),
        InputId::Gamepad(GamepadInputId::LeftStickRight),
    ),
    (
        InputId::Keyboard(Code::KeyZ),
        InputId::Gamepad(GamepadInputId::FPadDown),
    ),
    (
        InputId::Keyboard(Code::Enter),
        InputId::Gamepad(GamepadInputId::Start),
    ),
    (
        InputId::Keyboard(Code::ShiftRight),
        InputId::Gamepad(GamepadInputId::Select),
    ),
];
