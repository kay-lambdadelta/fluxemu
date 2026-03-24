use std::{
    collections::VecDeque,
    fmt::Debug,
    io::{Read, Write},
    sync::{
        Arc, Weak,
        atomic::{AtomicBool, Ordering},
    },
};

use arrayvec::ArrayVec;
use bitvec::{prelude::Lsb0, view::BitView};
use fluxemu_runtime::{
    component::{Component, ComponentConfig, ComponentVersion, LateContext, LateInitializedData},
    machine::{
        Machine,
        builder::{ComponentBuilder, SchedulerParticipation},
    },
    memory::{Address, AddressSpaceCache, AddressSpaceId},
    platform::Platform,
    scheduler::{Frequency, Period, SynchronizationContext},
};
use serde::{Deserialize, Serialize};

use crate::cycle::{BusMode, Cycle, MoveDestination, MoveSource, Phi1, Phi2, SetAddressBusSource};

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

pub const RESET_SEQUENCE_LENGTH: u32 = 6;

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

#[derive(Copy, Clone, PartialEq, Serialize, Deserialize, Debug, Default)]
/// We don't store this in memory bitpacked for performance reasons
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
        let mut byte = 0;
        let bits = byte.view_bits_mut::<Lsb0>();

        bits.set(7, self.negative);
        bits.set(6, self.overflow);
        bits.set(5, true);
        bits.set(4, break_);
        bits.set(3, self.decimal);
        bits.set(2, self.interrupt_disable);
        bits.set(1, self.zero);
        bits.set(0, self.carry);

        byte
    }

    pub fn from_byte(byte: u8) -> Self {
        let bits = byte.view_bits::<Lsb0>();

        Self {
            negative: bits[7],
            overflow: bits[6],
            decimal: bits[3],
            interrupt_disable: bits[2],
            zero: bits[1],
            carry: bits[0],
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
pub struct RdyFlag(AtomicBool);

impl Default for RdyFlag {
    fn default() -> Self {
        Self(AtomicBool::new(true))
    }
}

impl RdyFlag {
    pub fn load(&self) -> bool {
        self.0.load(Ordering::Acquire)
    }

    pub fn store(&self, value: bool) {
        self.0.store(value, Ordering::Release);
    }
}

#[derive(Debug)]
pub struct Mos6502 {
    a: u8,
    x: u8,
    y: u8,
    flags: FlagRegister,
    stack: u8,
    instruction_pointer: u16,
    instruction_queue: VecDeque<Cycle>,
    bus: Bus,
    effective_address: ArrayVec<u8, 2>,
    operand: u8,
    rdy: Arc<RdyFlag>,
    irq: Arc<IrqFlag>,
    nmi: Arc<NmiFlag>,
    config: Mos6502Config,
    address_space_cache: Option<AddressSpaceCache>,
    machine: Weak<Machine>,
    timestamp: Period,
    period: Period,
}

impl Component for Mos6502 {
    fn store_snapshot(&self, _writer: &mut dyn Write) -> Result<(), Box<dyn std::error::Error>> {
        todo!()
    }

    fn load_snapshot(
        &mut self,
        version: ComponentVersion,
        _reader: &mut dyn Read,
    ) -> Result<(), Box<dyn std::error::Error>> {
        match version {
            0 => {
                todo!()
            }
            other => Err(format!("Unsupported snapshot version: {other}").into()),
        }
    }

    fn synchronize(&mut self, mut context: SynchronizationContext) {
        let machine = self.machine.upgrade().unwrap();
        let address_space = machine
            .address_space(self.config.assigned_address_space)
            .unwrap();

        for now in context.allocate(self.period, None) {
            self.timestamp = now;

            let current_cycle = if let Some(cycle) = self.instruction_queue.pop_front() {
                cycle
            } else {
                Cycle::new(
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
                )
            };

            match current_cycle.phi1 {
                Some(Phi1::SetAddressBus {
                    source: SetAddressBusSource::InstructionPointer,
                }) => {
                    self.bus.address = self.instruction_pointer;
                }
                Some(Phi1::SetAddressBus {
                    source: SetAddressBusSource::EffectiveAddress,
                }) => {
                    match self.effective_address.len() {
                        1 => {
                            self.bus.address = u16::from(self.effective_address[0]);
                        }
                        2 => {
                            self.bus.address = u16::from_le_bytes([
                                self.effective_address[0],
                                self.effective_address[1],
                            ]);
                        }
                        _ => unreachable!(),
                    }

                    self.effective_address.clear();
                }
                Some(Phi1::SetAddressBus {
                    source: SetAddressBusSource::Constant(value),
                }) => {
                    self.bus.address = value;
                }
                Some(Phi1::SetAddressBus {
                    source: SetAddressBusSource::Stack,
                }) => {
                    self.bus.address = u16::from(self.stack) | STACK_BASE_ADDRESS;
                }
                None => {}
            }

            let is_read_cycle = match current_cycle.bus_mode {
                BusMode::Read => {
                    self.bus.data = address_space
                        .read_le_value(
                            self.bus.address as Address,
                            self.timestamp,
                            self.address_space_cache.as_mut(),
                        )
                        .unwrap_or_default();

                    true
                }
                BusMode::Write => false,
            };

            if self.rdy.load() || !is_read_cycle {
                self.handle_phi2(&current_cycle);

                match current_cycle.bus_mode {
                    BusMode::Read => {}
                    BusMode::Write => {
                        address_space
                            .write_le_value(
                                self.bus.address as Address,
                                self.timestamp,
                                self.address_space_cache.as_mut(),
                                self.bus.data,
                            )
                            .unwrap_or_default();
                    }
                }

                tracing::trace!("Current cycle {:x?}, State {:x?}", current_cycle, self);

                // Check for interrupts

                if self.config.kind.supports_interrupts()
                    && self.instruction_queue.is_empty()
                    && self.nmi.interrupt_required()
                {
                    self.handle_nmi();
                }
            } else {
                self.instruction_queue.push_front(current_cycle);
            }
        }
    }

    fn needs_work(&self, delta: Period) -> bool {
        delta >= self.period
    }
}

impl<P: Platform> ComponentConfig<P> for Mos6502Config {
    type Component = Mos6502;

    fn late_initialize(
        component: &mut Self::Component,
        data: &LateContext<P>,
    ) -> LateInitializedData<P> {
        component.machine = Arc::downgrade(&data.machine);
        component.address_space_cache = Some(
            data.machine
                .address_space(component.config.assigned_address_space)
                .unwrap()
                .create_cache(),
        );

        LateInitializedData::default()
    }

    fn build_component(
        self,
        component_builder: ComponentBuilder<'_, '_, P, Self::Component>,
    ) -> Result<Self::Component, Box<dyn std::error::Error>> {
        component_builder.scheduler_participation(SchedulerParticipation::SchedulerDriven);

        let mut component = Mos6502 {
            a: 0,
            x: 0,
            y: 0,
            flags: FlagRegister::default(),
            stack: 0xff,
            // Will be set later
            instruction_pointer: 0x0000,
            instruction_queue: VecDeque::default(),
            operand: 0,
            bus: Bus {
                address: 0x0000,
                data: 0x00,
            },
            effective_address: ArrayVec::default(),
            rdy: Arc::default(),
            irq: Arc::default(),
            nmi: Arc::default(),
            machine: Weak::default(),
            address_space_cache: None,
            period: self.frequency.recip(),
            config: self,
            timestamp: Period::default(),
        };

        // Put it in the reset state for startup
        component.reset();

        Ok(component)
    }
}

#[derive(Debug)]
pub struct IrqFlag(AtomicBool);

impl Default for IrqFlag {
    fn default() -> Self {
        Self(AtomicBool::new(true))
    }
}

impl IrqFlag {
    pub fn store(&self, irq: bool) {
        self.0.store(irq, Ordering::Release);
    }

    pub fn interrupt_required(&self) -> bool {
        !self.0.load(Ordering::Acquire)
    }
}

/// NMI is falling edge
#[derive(Debug)]
pub struct NmiFlag {
    current_state: AtomicBool,
    falling_edge_occured: AtomicBool,
}

impl Default for NmiFlag {
    fn default() -> Self {
        Self {
            current_state: AtomicBool::new(true),
            falling_edge_occured: AtomicBool::new(false),
        }
    }
}

impl NmiFlag {
    pub fn store(&self, nmi: bool) {
        if self.current_state.swap(nmi, Ordering::AcqRel) && !nmi {
            self.falling_edge_occured.store(true, Ordering::Release);
        }
    }

    pub fn interrupt_required(&self) -> bool {
        self.falling_edge_occured.swap(false, Ordering::AcqRel)
    }
}

impl Mos6502 {
    pub fn rdy(&self) -> Arc<RdyFlag> {
        self.rdy.clone()
    }

    pub fn irq(&self) -> Arc<IrqFlag> {
        self.irq.clone()
    }

    pub fn nmi(&self) -> Arc<NmiFlag> {
        self.nmi.clone()
    }

    pub fn address_space(&self) -> AddressSpaceId {
        self.config.assigned_address_space
    }

    fn reset(&mut self) {
        self.instruction_queue.clear();
        self.instruction_queue.extend([
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
        self.instruction_queue.extend([
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
}
