use alloc::{boxed::Box, sync::Arc};

use fluxemu_input::{DirectionPart, GamepadInputId, InputId, InputShape};
use fluxemu_platform::Platform;
use fluxemu_runtime::{
    component::{Component, config::ComponentConfig},
    input::{GuestInput, GuestInputDevice},
    machine::builder::ComponentBuilder,
    persistence::PersistentComponentState,
};
use keyboard_types::Code;
use serde::{Deserialize, Serialize};
use strum::{EnumIter, IntoEnumIterator};

use crate::gamepad::port_device::PortDevice;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, EnumIter)]
pub enum StandardControllerButton {
    A,
    B,
    Select,
    Start,
    DPad(DirectionPart),
}

impl GuestInput for StandardControllerButton {
    fn all() -> impl Iterator<Item = Self> {
        Self::iter()
    }

    fn shape(&self) -> InputShape {
        match self {
            Self::DPad(_) => InputShape::Direction,
            _ => InputShape::Button,
        }
    }

    fn split(self) -> (Self, DirectionPart) {
        match self {
            Self::DPad(part) => (Self::DPad(DirectionPart::All), part),
            whole => (whole, DirectionPart::All),
        }
    }
}

const REPORT_ORDER: [StandardControllerButton; 8] = [
    StandardControllerButton::A,
    StandardControllerButton::B,
    StandardControllerButton::Select,
    StandardControllerButton::Start,
    StandardControllerButton::DPad(DirectionPart::Up),
    StandardControllerButton::DPad(DirectionPart::Down),
    StandardControllerButton::DPad(DirectionPart::Left),
    StandardControllerButton::DPad(DirectionPart::Right),
];

const DEFAULT_MAPPINGS: [(InputId, StandardControllerButton); 13] = [
    (
        InputId::Gamepad(GamepadInputId::FPadDown),
        StandardControllerButton::A,
    ),
    (
        InputId::Gamepad(GamepadInputId::FPadRight),
        StandardControllerButton::B,
    ),
    (
        InputId::Gamepad(GamepadInputId::Select),
        StandardControllerButton::Select,
    ),
    (
        InputId::Gamepad(GamepadInputId::Start),
        StandardControllerButton::Start,
    ),
    (
        InputId::Gamepad(GamepadInputId::DPad(DirectionPart::All)),
        StandardControllerButton::DPad(DirectionPart::All),
    ),
    (InputId::Keyboard(Code::KeyZ), StandardControllerButton::A),
    (InputId::Keyboard(Code::KeyX), StandardControllerButton::B),
    (
        InputId::Keyboard(Code::ShiftRight),
        StandardControllerButton::Select,
    ),
    (
        InputId::Keyboard(Code::Enter),
        StandardControllerButton::Start,
    ),
    (
        InputId::Keyboard(Code::ArrowUp),
        StandardControllerButton::DPad(DirectionPart::Up),
    ),
    (
        InputId::Keyboard(Code::ArrowDown),
        StandardControllerButton::DPad(DirectionPart::Down),
    ),
    (
        InputId::Keyboard(Code::ArrowLeft),
        StandardControllerButton::DPad(DirectionPart::Left),
    ),
    (
        InputId::Keyboard(Code::ArrowRight),
        StandardControllerButton::DPad(DirectionPart::Right),
    ),
];

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct StandardControllerState {
    current_read: u8,
    strobe: bool,
}

#[derive(Debug)]
pub struct StandardController {
    input_device: Arc<GuestInputDevice<StandardControllerButton>>,
    state: StandardControllerState,
}

impl PortDevice for StandardController {
    fn set_strobe(&mut self, strobe: bool) {
        self.state.strobe = strobe;

        if strobe {
            self.state.current_read = 0;
        }
    }

    fn read(&mut self, avoid_side_effects: bool) -> u8 {
        let Some(&button) = REPORT_ORDER.get(usize::from(self.state.current_read)) else {
            return 1;
        };

        let pressed = self.input_device.is_held(button);

        if !avoid_side_effects && !self.state.strobe {
            self.state.current_read += 1;
        }

        pressed as u8
    }
}

impl PersistentComponentState<StandardController> for StandardControllerState {
    fn capture(component: &StandardController) -> impl Serialize {
        &component.state
    }

    fn apply(self, component: &mut StandardController) {
        component.state = self;
    }
}

impl Component for StandardController {
    type Event = ();
    type Save = ();
    type Snapshot = StandardControllerState;
}

impl<P: Platform> ComponentConfig<P> for StandardControllerConfig {
    type Component = StandardController;

    fn build_component(
        self,
        component_builder: ComponentBuilder<P, Self::Component>,
    ) -> Result<Self::Component, Box<dyn core::error::Error>> {
        let (_, input_device) = component_builder.input("gamepad", DEFAULT_MAPPINGS);

        Ok(StandardController {
            input_device,
            state: StandardControllerState::default(),
        })
    }
}

#[derive(Debug)]
pub struct StandardControllerConfig;
