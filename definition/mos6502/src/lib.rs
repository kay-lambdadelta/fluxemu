use std::{
    fmt::Debug,
    io::{Read, Write},
};

use fluxemu_runtime::{
    ComponentPath, ComponentRuntimeApi,
    component::{Component, config::ComponentConfig},
    event::{Event, downcast_event},
    machine::builder::{ComponentBuilder, SchedulerParticipation},
    memory::{Address, AddressSpaceId},
    persistence::PersistanceFormatVersion,
    platform::Platform,
    scheduler::{Frequency, Period, SynchronizationContext},
};
use serde::{Deserialize, Serialize};

use crate::cycle::{
    BusMode, Cycle, Flag, MoveDestination, MoveSource, Phi1, Phi2, SetAddressBusSource,
};

mod cycle;
mod decoder;
mod handle_phi2;
mod instruction;

pub const RESET_VECTOR: u16 = 0xfffc;
pub const IRQ_VECTOR: u16 = 0xfffe;
pub const NMI_VECTOR: u16 = 0xfffa;
pub const PAGE_SIZE: usize = 256;
pub const STACK_BASE_ADDRESS: u16 = 0x0100;
pub const INTERRUPT_VECTOR: u16 = 0xfffe;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Bus {
    pub address: u16,
    pub data: u8,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum Mos6502Kind {
    /// Standard
    Mos6502,
    /// Slimmed down atari 2600 version
    Mos6507,
    /// NES version
    Ricoh2A0x,
    // Upgraded version
    Wdc65C02,
}

impl Mos6502Kind {
    #[inline]
    pub fn original_instruction_set(&self) -> bool {
        matches!(self, Self::Mos6502 | Self::Mos6507 | Self::Ricoh2A0x)
    }

    #[inline]
    pub fn supports_decimal(&self) -> bool {
        !matches!(self, Mos6502Kind::Ricoh2A0x)
    }

    #[inline]
    pub fn supports_interrupts(&self) -> bool {
        !matches!(self, Mos6502Kind::Mos6507)
    }

    #[inline]
    pub fn has_absolute_indirect_page_wrap_errata(&self) -> bool {
        matches!(self, Self::Mos6502 | Self::Mos6507 | Self::Ricoh2A0x)
    }
}

/// We don't store this in memory bitpacked for performance reasons
#[derive(Copy, Clone, PartialEq, Serialize, Deserialize, Debug, Default)]
pub struct FlagRegister {
    negative: bool,
    overflow: bool,
    decimal: bool,
    interrupt_disable: bool,
    zero: bool,
    carry: bool,
}

impl FlagRegister {
    pub fn to_byte(&self, break_: bool) -> u8 {
        (self.negative as u8) << 7
            | (self.overflow as u8) << 6
            | 1 << 5
            | (break_ as u8) << 4
            | (self.decimal as u8) << 3
            | (self.interrupt_disable as u8) << 2
            | (self.zero as u8) << 1
            | (self.carry as u8)
    }

    pub fn from_byte(byte: u8) -> Self {
        Self {
            negative: (byte >> 7) & 0b0000_0001 != 0,
            overflow: (byte >> 6) & 0b0000_0001 != 0,
            decimal: (byte >> 3) & 0b0000_0001 != 0,
            interrupt_disable: (byte >> 2) & 0b0000_0001 != 0,
            zero: (byte >> 1) & 0b0000_0001 != 0,
            carry: byte & 1 != 0,
        }
    }
}

#[derive(Debug)]
pub struct Mos6502Config {
    pub frequency: Frequency,
    pub assigned_address_space: AddressSpaceId,
    pub kind: Mos6502Kind,
    pub broken_ror: bool,
}

#[derive(Debug, Serialize, Deserialize)]
struct State {
    a: u8,
    x: u8,
    y: u8,
    flags: FlagRegister,
    stack: u8,
    instruction_pointer: u16,
    cycle_queue: heapless::Deque<Cycle, 8>,
    bus: Bus,
    effective_address: heapless::Vec<u8, 2>,
    consume_effective_address: bool,
    operand: u8,
    rdy: bool,
    nmi: NmiFlag,
    irq: bool,
}

#[derive(Debug)]
pub struct Mos6502 {
    state: State,
    config: Mos6502Config,
    period: Period,
    path: ComponentPath,
}

impl Component for Mos6502 {
    type Event = Mos6502Event;

    fn store_snapshot(&self, writer: &mut dyn Write) -> Result<(), Box<dyn std::error::Error>> {
        rmp_serde::encode::write(writer, &self.state)?;

        Ok(())
    }

    fn load_snapshot(
        &mut self,
        _version: PersistanceFormatVersion,
        reader: &mut dyn Read,
    ) -> Result<(), Box<dyn std::error::Error>> {
        self.state = rmp_serde::from_read(reader)?;

        Ok(())
    }

    fn synchronize(&mut self, mut context: SynchronizationContext) {
        let runtime = ComponentRuntimeApi::current(self.path.clone());

        let mut address_space = runtime
            .address_space(self.config.assigned_address_space)
            .unwrap();

        for timestamp in context.allocate(self.period) {
            if self.state.cycle_queue.is_empty() {
                self.state
                    .cycle_queue
                    .push_back(Cycle::new(
                        BusMode::Read,
                        Some(Phi1::SetAddressBus {
                            source: SetAddressBusSource::InstructionPointer,
                        }),
                        [
                            Phi2::IncrementInstructionPointer,
                            Phi2::Move {
                                source: MoveSource::Data,
                                destination: MoveDestination::Opcode,
                            },
                        ],
                    ))
                    .unwrap();
            }

            let current_cycle = self.state.cycle_queue.front_mut().unwrap();

            match current_cycle.phi1 {
                Some(Phi1::SetAddressBus {
                    source: SetAddressBusSource::InstructionPointer,
                }) => {
                    self.state.bus.address = self.state.instruction_pointer;
                }
                Some(Phi1::SetAddressBus {
                    source: SetAddressBusSource::EffectiveAddress,
                }) => {
                    match self.state.effective_address.len() {
                        1 => {
                            self.state.bus.address = u16::from(self.state.effective_address[0]);
                        }
                        2 => {
                            self.state.bus.address = u16::from_le_bytes([
                                self.state.effective_address[0],
                                self.state.effective_address[1],
                            ]);
                        }
                        _ => unreachable!(),
                    }

                    self.state.consume_effective_address = true;
                }
                Some(Phi1::SetAddressBus {
                    source: SetAddressBusSource::Constant(value),
                }) => {
                    self.state.bus.address = value;
                }
                Some(Phi1::SetAddressBus {
                    source: SetAddressBusSource::Stack,
                }) => {
                    self.state.bus.address = u16::from(self.state.stack) | STACK_BASE_ADDRESS;
                }
                None => {}
            }

            let is_read_cycle = match current_cycle.bus_mode {
                BusMode::Read => {
                    self.state.bus.data = address_space
                        .read_le_value(self.state.bus.address as Address, timestamp)
                        .unwrap_or_default();

                    true
                }
                BusMode::Write => false,
            };

            if self.state.rdy || !is_read_cycle {
                if std::mem::take(&mut self.state.consume_effective_address) {
                    self.state.effective_address.clear();
                }

                let current_cycle = self.state.cycle_queue.pop_front().unwrap();

                self.handle_phi2(&current_cycle);

                match current_cycle.bus_mode {
                    BusMode::Read => {}
                    BusMode::Write => {
                        address_space
                            .write_le_value(
                                self.state.bus.address as Address,
                                timestamp,
                                self.state.bus.data,
                            )
                            .unwrap_or_default();
                    }
                }

                // Check for interrupts

                if self.config.kind.supports_interrupts() && self.state.cycle_queue.is_empty() {
                    if self.state.nmi.interrupt_required() {
                        self.handle_nmi();
                    } else if std::mem::take(&mut self.state.irq) {
                        self.handle_irq();
                    }
                }
            }
        }
    }

    fn needs_work(&self, _timestamp: &Period, delta: &Period) -> bool {
        delta >= &self.period
    }

    fn handle_event(&mut self, event: Box<dyn Event>) {
        let event = downcast_event::<Self>(event);

        match event {
            Mos6502Event::FlagChange { pin: flag, value } => match flag {
                Pin::Nmi => self.state.nmi.store(value),
                Pin::Irq => self.state.irq = value,
                Pin::Rdy => self.state.rdy = value,
            },
        }
    }
}

impl<P: Platform> ComponentConfig<P> for Mos6502Config {
    type Component = Mos6502;
    const CURRENT_SNAPSHOT_VERSION: PersistanceFormatVersion = 0;

    fn build_component(
        self,
        component_builder: ComponentBuilder<'_, '_, P, Self::Component>,
    ) -> Result<Self::Component, Box<dyn std::error::Error>> {
        let component_builder = component_builder
            .scheduler_participation(Some(SchedulerParticipation::SchedulerDriven));

        let mut component = Mos6502 {
            state: State {
                a: 0,
                x: 0,
                y: 0,
                flags: FlagRegister::default(),
                stack: 0xff,
                // Will be set later
                instruction_pointer: 0x0000,
                cycle_queue: heapless::Deque::default(),
                operand: 0,
                bus: Bus {
                    address: 0x0000,
                    data: 0x00,
                },
                rdy: true,
                irq: false,
                nmi: NmiFlag::default(),
                effective_address: heapless::Vec::default(),
                consume_effective_address: false,
            },
            period: self.frequency.recip(),
            path: component_builder.path().clone(),
            config: self,
        };

        // Put it in the reset state for startup
        component.reset();

        Ok(component)
    }
}

/// NMI is falling edge
#[derive(Debug, Serialize, Deserialize, Clone, Copy)]
struct NmiFlag {
    current_state: bool,
    falling_edge_occured: bool,
}

impl Default for NmiFlag {
    fn default() -> Self {
        Self {
            current_state: true,
            falling_edge_occured: false,
        }
    }
}

impl NmiFlag {
    pub fn store(&mut self, nmi: bool) {
        if std::mem::replace(&mut self.current_state, nmi) && !nmi {
            self.falling_edge_occured = true;
        }
    }

    pub fn interrupt_required(&mut self) -> bool {
        std::mem::take(&mut self.falling_edge_occured)
    }
}

impl Mos6502 {
    pub fn address_space(&self) -> AddressSpaceId {
        self.config.assigned_address_space
    }

    fn reset(&mut self) {
        self.state.cycle_queue.clear();
        self.state.cycle_queue.extend([
            // Two dummy cycles
            Cycle::new(BusMode::Read, None, []),
            Cycle::new(BusMode::Read, None, []),
            // Initialize the stack
            Cycle::new(
                BusMode::Read,
                None,
                [Phi2::Move {
                    source: MoveSource::Constant(0xfd),
                    destination: MoveDestination::Stack,
                }],
            ),
            // Sets flags
            Cycle::new(
                BusMode::Read,
                None,
                [Phi2::Move {
                    source: MoveSource::Constant(
                        FlagRegister {
                            negative: false,
                            overflow: false,
                            decimal: false,
                            interrupt_disable: true,
                            zero: false,
                            carry: false,
                        }
                        .to_byte(false),
                    ),
                    destination: MoveDestination::Flags,
                }],
            ),
            // Load the reset vector
            Cycle::new(
                BusMode::Read,
                Some(Phi1::SetAddressBus {
                    source: SetAddressBusSource::Constant(RESET_VECTOR),
                }),
                [Phi2::Move {
                    source: MoveSource::Data,
                    destination: MoveDestination::EffectiveAddress,
                }],
            ),
            Cycle::new(
                BusMode::Read,
                Some(Phi1::SetAddressBus {
                    source: SetAddressBusSource::Constant(RESET_VECTOR + 1),
                }),
                [
                    Phi2::Move {
                        source: MoveSource::Data,
                        destination: MoveDestination::EffectiveAddress,
                    },
                    Phi2::LoadInstructionPointerFromEffectiveAddress,
                ],
            ),
        ]);
    }

    fn handle_nmi(&mut self) {
        self.state.cycle_queue.extend([
            Cycle::new(
                BusMode::Read,
                Some(Phi1::SetAddressBus {
                    source: SetAddressBusSource::InstructionPointer,
                }),
                [],
            ),
            Cycle::new(
                BusMode::Write,
                Some(Phi1::SetAddressBus {
                    source: SetAddressBusSource::Stack,
                }),
                [
                    Phi2::Move {
                        source: MoveSource::InstructionPointer { offset: 1 },
                        destination: MoveDestination::Data,
                    },
                    Phi2::IncrementStack { subtract: true },
                ],
            ),
            Cycle::new(
                BusMode::Write,
                Some(Phi1::SetAddressBus {
                    source: SetAddressBusSource::Stack,
                }),
                [
                    Phi2::Move {
                        source: MoveSource::InstructionPointer { offset: 0 },
                        destination: MoveDestination::Data,
                    },
                    Phi2::IncrementStack { subtract: true },
                ],
            ),
            Cycle::new(
                BusMode::Write,
                Some(Phi1::SetAddressBus {
                    source: SetAddressBusSource::Stack,
                }),
                [
                    Phi2::Move {
                        source: MoveSource::Flags { break_: false },
                        destination: MoveDestination::Data,
                    },
                    Phi2::IncrementStack { subtract: true },
                ],
            ),
            Cycle::new(
                BusMode::Read,
                Some(Phi1::SetAddressBus {
                    source: SetAddressBusSource::Constant(NMI_VECTOR),
                }),
                [Phi2::Move {
                    source: MoveSource::Data,
                    destination: MoveDestination::EffectiveAddress,
                }],
            ),
            Cycle::new(
                BusMode::Read,
                Some(Phi1::SetAddressBus {
                    source: SetAddressBusSource::Constant(NMI_VECTOR + 1),
                }),
                [
                    Phi2::Move {
                        source: MoveSource::Data,
                        destination: MoveDestination::EffectiveAddress,
                    },
                    Phi2::LoadInstructionPointerFromEffectiveAddress,
                ],
            ),
        ]);
    }

    fn handle_irq(&mut self) {
        self.state.cycle_queue.extend([
            Cycle::new(
                BusMode::Read,
                Some(Phi1::SetAddressBus {
                    source: SetAddressBusSource::InstructionPointer,
                }),
                [],
            ),
            Cycle::new(
                BusMode::Read,
                Some(Phi1::SetAddressBus {
                    source: SetAddressBusSource::InstructionPointer,
                }),
                [],
            ),
            Cycle::new(
                BusMode::Write,
                Some(Phi1::SetAddressBus {
                    source: SetAddressBusSource::Stack,
                }),
                [
                    Phi2::Move {
                        source: MoveSource::InstructionPointer { offset: 1 },
                        destination: MoveDestination::Data,
                    },
                    Phi2::IncrementStack { subtract: true },
                ],
            ),
            Cycle::new(
                BusMode::Write,
                Some(Phi1::SetAddressBus {
                    source: SetAddressBusSource::Stack,
                }),
                [
                    Phi2::Move {
                        source: MoveSource::InstructionPointer { offset: 0 },
                        destination: MoveDestination::Data,
                    },
                    Phi2::IncrementStack { subtract: true },
                ],
            ),
            Cycle::new(
                BusMode::Write,
                Some(Phi1::SetAddressBus {
                    source: SetAddressBusSource::Stack,
                }),
                [
                    Phi2::Move {
                        source: MoveSource::Flags { break_: false },
                        destination: MoveDestination::Data,
                    },
                    Phi2::IncrementStack { subtract: true },
                ],
            ),
            Cycle::new(
                BusMode::Read,
                Some(Phi1::SetAddressBus {
                    source: SetAddressBusSource::Constant(IRQ_VECTOR),
                }),
                [Phi2::Move {
                    source: MoveSource::Data,
                    destination: MoveDestination::EffectiveAddress,
                }],
            ),
            Cycle::new(
                BusMode::Read,
                Some(Phi1::SetAddressBus {
                    source: SetAddressBusSource::Constant(IRQ_VECTOR + 1),
                }),
                [
                    Phi2::Move {
                        source: MoveSource::Data,
                        destination: MoveDestination::EffectiveAddress,
                    },
                    Phi2::LoadInstructionPointerFromEffectiveAddress,
                    Phi2::SetFlag {
                        flag: Flag::InterruptDisable,
                        value: true,
                    },
                ],
            ),
        ]);
    }
}

#[derive(Debug, Clone)]
pub enum Pin {
    Nmi,
    Irq,
    Rdy,
}

#[derive(Debug, Clone)]
pub enum Mos6502Event {
    FlagChange { pin: Pin, value: bool },
}
