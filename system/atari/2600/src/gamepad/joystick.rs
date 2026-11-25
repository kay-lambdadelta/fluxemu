use std::sync::Arc;

use fluxemu_definition_mos6532::Register;
use fluxemu_input::{DirectionPart, GamepadInputId, InputId, InputShape};
use fluxemu_platform::Platform;
use fluxemu_runtime::{
    component::{Component, config::ComponentConfig},
    input::{GuestInput, GuestInputDevice},
    machine::builder::ComponentBuilder,
    memory::{Address, AddressSpaceId},
};
use keyboard_types::Code;
use serde::{Deserialize, Serialize};
use strum::{EnumIter, IntoEnumIterator};

const IDLE_CONSOLE_SWITCHES: u8 = 0b0000_1000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, EnumIter)]
pub enum JoystickInput {
    Stick(DirectionPart),
    Fire,
    Reset,
    Select,
}

impl GuestInput for JoystickInput {
    fn all() -> impl Iterator<Item = Self> {
        Self::iter()
    }

    fn shape(&self) -> InputShape {
        match self {
            Self::Stick(_) => InputShape::Direction,
            _ => InputShape::Button,
        }
    }

    fn split(self) -> (Self, DirectionPart) {
        match self {
            Self::Stick(part) => (Self::Stick(DirectionPart::All), part),
            whole => (whole, DirectionPart::All),
        }
    }
}

#[derive(Debug)]
pub struct Joystick {
    players: [Arc<GuestInputDevice<JoystickInput>>; 2],
    riot_registers_address: Address,
}

impl Joystick {
    pub fn fire_pressed(&self, player: usize) -> bool {
        self.players[player].is_held(JoystickInput::Fire)
    }

    fn console_switches(&self) -> u8 {
        let any_player_holds = |input| self.players.iter().any(|player| player.is_held(input));

        let reset = any_player_holds(JoystickInput::Reset);
        let select = any_player_holds(JoystickInput::Select);

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
fn direction_nibble(device: &GuestInputDevice<JoystickInput>) -> u8 {
    (device.is_held(JoystickInput::Stick(DirectionPart::Up)) as u8)
        | (device.is_held(JoystickInput::Stick(DirectionPart::Down)) as u8) << 1
        | (device.is_held(JoystickInput::Stick(DirectionPart::Left)) as u8) << 2
        | (device.is_held(JoystickInput::Stick(DirectionPart::Right)) as u8) << 3
}

impl<P: Platform> ComponentConfig<P> for JoystickConfig {
    type Component = Joystick;

    fn build_component(
        self,
        component_builder: ComponentBuilder<P, Self::Component>,
    ) -> Result<Self::Component, Box<dyn std::error::Error>> {
        let (component_builder, player1) = component_builder.input("player-1", DEFAULT_MAPPINGS);
        let (_, player2) = component_builder.input("player-2", DEFAULT_MAPPINGS);

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

const DEFAULT_MAPPINGS: [(InputId, JoystickInput); 12] = [
    (
        InputId::Gamepad(GamepadInputId::LeftStick(DirectionPart::All)),
        JoystickInput::Stick(DirectionPart::All),
    ),
    (
        InputId::Gamepad(GamepadInputId::DPad(DirectionPart::All)),
        JoystickInput::Stick(DirectionPart::All),
    ),
    (
        InputId::Gamepad(GamepadInputId::FPadDown),
        JoystickInput::Fire,
    ),
    (
        InputId::Gamepad(GamepadInputId::Start),
        JoystickInput::Reset,
    ),
    (
        InputId::Gamepad(GamepadInputId::Select),
        JoystickInput::Select,
    ),
    (
        InputId::Keyboard(Code::ArrowDown),
        JoystickInput::Stick(DirectionPart::Down),
    ),
    (
        InputId::Keyboard(Code::ArrowUp),
        JoystickInput::Stick(DirectionPart::Up),
    ),
    (
        InputId::Keyboard(Code::ArrowLeft),
        JoystickInput::Stick(DirectionPart::Left),
    ),
    (
        InputId::Keyboard(Code::ArrowRight),
        JoystickInput::Stick(DirectionPart::Right),
    ),
    (InputId::Keyboard(Code::KeyZ), JoystickInput::Fire),
    (InputId::Keyboard(Code::Enter), JoystickInput::Reset),
    (InputId::Keyboard(Code::ShiftRight), JoystickInput::Select),
];
