use std::{
    io::{Read, Write},
    marker::PhantomData,
    sync::{Arc, Mutex, Weak},
};

use arrayvec::ArrayVec;
use fluxemu_runtime::{
    component::{
        Component, ComponentConfig, ComponentVersion, LateContext, LateInitializedData,
        TypedComponentHandle,
    },
    input::LogicalInputDevice,
    machine::{
        Machine,
        builder::{ComponentBuilder, SchedulerParticipation},
    },
    memory::AddressSpaceId,
    path::ComponentPath,
    platform::Platform,
    scheduler::{Frequency, Period, SynchronizationContext},
};
use input::Chip8KeyCode;
use instruction::Register;
use serde::{Deserialize, Serialize};

use super::Chip8Mode;
use crate::{
    audio::Chip8Audio,
    display::{Chip8Display, SupportedGraphicsApiChip8Display},
    processor::{
        decoder::decode_instruction,
        input::{DEFAULT_MAPPINGS, PRESENT_INPUTS},
    },
    timer::Chip8Timer,
};

pub mod decoder;
mod input;
mod instruction;
mod interpret;

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
enum ExecutionState {
    Normal,
    AwaitingKeyPress {
        register: Register,
    },
    // KeyQuery does not return on key press but on key release, contrary to some documentation
    AwaitingKeyRelease {
        register: Register,
        keys: Vec<Chip8KeyCode>,
    },
    AwaitingVsync,
    Halted,
}

// This is extremely complex because the chip8 cpu has a lot of non cpu
// machinery

#[derive(Debug, Deserialize, Serialize, Clone)]
struct Chip8ProcessorRegisters {
    work_registers: [u8; 16],
    index: u16,
    program: u16,
}

impl Default for Chip8ProcessorRegisters {
    fn default() -> Self {
        Self {
            work_registers: [0; 16],
            index: 0,
            program: 0x200,
        }
    }
}

#[derive(Debug)]
pub struct ProcessorState {
    registers: Chip8ProcessorRegisters,
    stack: ArrayVec<u16, 16>,
    execution_state: ExecutionState,
}

impl Default for ProcessorState {
    fn default() -> Self {
        Self {
            stack: ArrayVec::default(),
            registers: Chip8ProcessorRegisters::default(),
            execution_state: ExecutionState::Normal,
        }
    }
}

#[derive(Debug)]
pub struct Chip8Processor<G: SupportedGraphicsApiChip8Display> {
    state: ProcessorState,
    /// Keypad virtual gamepad
    keypad: Arc<LogicalInputDevice>,
    // What chip8 mode we are currently in
    mode: Arc<Mutex<Chip8Mode>>,
    display: TypedComponentHandle<Chip8Display<G>>,
    audio: TypedComponentHandle<Chip8Audio>,
    timer: TypedComponentHandle<Chip8Timer>,
    config: Chip8ProcessorConfig<G>,
    machine: Weak<Machine>,
    timestamp: Period,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Chip8ProcessorSnapshot {
    registers: Chip8ProcessorRegisters,
    stack: ArrayVec<u16, 16>,
    execution_state: ExecutionState,
}

impl<G: SupportedGraphicsApiChip8Display> Component for Chip8Processor<G> {
    fn load_snapshot(
        &mut self,
        version: ComponentVersion,
        reader: &mut dyn Read,
    ) -> Result<(), Box<dyn std::error::Error>> {
        assert_eq!(version, 0);

        let snapshot: Chip8ProcessorSnapshot = rmp_serde::decode::from_read(reader)?;

        self.state.registers = snapshot.registers;
        self.state.stack = snapshot.stack;
        self.state.execution_state = snapshot.execution_state;

        Ok(())
    }

    fn store_snapshot(&self, mut writer: &mut dyn Write) -> Result<(), Box<dyn std::error::Error>> {
        let snapshot = Chip8ProcessorSnapshot {
            registers: self.state.registers.clone(),
            stack: self.state.stack.clone(),
            execution_state: self.state.execution_state.clone(),
        };

        rmp_serde::encode::write_named(&mut writer, &snapshot)?;

        Ok(())
    }

    fn synchronize(&mut self, mut context: SynchronizationContext) {
        let machine = self.machine.upgrade().unwrap();
        let address_space = machine
            .address_space(self.config.cpu_address_space)
            .unwrap();

        for now in context.allocate(self.config.frequency.recip(), None) {
            self.timestamp = now;

            'main: {
                match &self.state.execution_state {
                    ExecutionState::Normal => {
                        let mut instruction = [0; 2];

                        address_space
                            .read(
                                self.state.registers.program as usize,
                                self.timestamp,
                                None,
                                &mut instruction,
                            )
                            .unwrap();

                        let instruction =
                            decode_instruction(instruction).expect("Failed to decode instruction");

                        self.state.registers.program = self.state.registers.program.wrapping_add(2);

                        self.interpret_instruction(address_space, instruction);
                    }
                    ExecutionState::AwaitingKeyPress { register } => {
                        let mut pressed = Vec::new();

                        // Go through every chip8 key
                        for key in 0x0..0xf {
                            let keycode = Chip8KeyCode(key);

                            if self
                                .keypad
                                .get_state(keycode.try_into().unwrap())
                                .as_digital(None)
                            {
                                pressed.push(keycode);
                            }
                        }

                        if !pressed.is_empty() {
                            self.state.execution_state = ExecutionState::AwaitingKeyRelease {
                                register: *register,
                                keys: pressed,
                            };

                            break 'main;
                        }
                    }
                    ExecutionState::AwaitingKeyRelease { register, keys } => {
                        for key_code in keys {
                            if !self
                                .keypad
                                .get_state((*key_code).try_into().unwrap())
                                .as_digital(None)
                            {
                                let register = *register;
                                self.state.registers.work_registers[register as usize] = key_code.0;
                                self.state.execution_state = ExecutionState::Normal;
                                break 'main;
                            }
                        }
                    }
                    ExecutionState::AwaitingVsync => {
                        let vsync_occured = self
                            .display
                            .interact(self.timestamp, |component| component.vsync_occurred);

                        if vsync_occured {
                            self.state.execution_state = ExecutionState::Normal;
                            break 'main;
                        }
                    }
                    ExecutionState::Halted => {
                        // Do nothing
                    }
                }
            }
        }
    }

    fn needs_work(&self, delta: Period) -> bool {
        delta >= self.config.frequency.recip()
    }
}

#[derive(Debug)]
pub struct Chip8ProcessorConfig<G: SupportedGraphicsApiChip8Display> {
    pub cpu_address_space: AddressSpaceId,
    pub display: ComponentPath,
    pub audio: ComponentPath,
    pub timer: ComponentPath,
    pub frequency: Frequency,
    pub force_mode: Option<Chip8Mode>,
    pub always_shr_in_place: bool,
    pub _phantom: PhantomData<fn() -> G>,
}

impl<P: Platform<GraphicsApi: SupportedGraphicsApiChip8Display>> ComponentConfig<P>
    for Chip8ProcessorConfig<P::GraphicsApi>
{
    type Component = Chip8Processor<P::GraphicsApi>;

    fn late_initialize(
        component: &mut Self::Component,
        data: &LateContext<P>,
    ) -> LateInitializedData<P> {
        component.machine = Arc::downgrade(&data.machine);

        LateInitializedData::default()
    }

    fn build_component(
        self,
        component_builder: ComponentBuilder<'_, '_, P, Self::Component>,
    ) -> Result<Self::Component, Box<dyn std::error::Error>> {
        let mode = Arc::new(Mutex::new(self.force_mode.unwrap_or(Chip8Mode::Chip8)));
        let state = ProcessorState::default();

        let (component_builder, keypad) = component_builder
            .scheduler_participation(SchedulerParticipation::SchedulerDriven)
            .input("keypad", PRESENT_INPUTS, DEFAULT_MAPPINGS);

        Ok(Chip8Processor {
            state,
            keypad,
            mode,
            display: component_builder
                .typed_component_handle(&self.display)
                .unwrap(),
            audio: component_builder
                .typed_component_handle(&self.audio)
                .unwrap(),
            timer: component_builder
                .typed_component_handle(&self.timer)
                .unwrap(),
            machine: Weak::default(),
            config: self,
            timestamp: Period::default(),
        })
    }
}
