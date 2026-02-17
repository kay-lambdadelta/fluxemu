use std::{
    collections::VecDeque,
    fmt::Debug,
    io::{Read, Write},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};

use arrayvec::ArrayVec;
use bitvec::{prelude::Lsb0, view::BitView};
use fluxemu_runtime::{
    component::{Component, ComponentConfig, ComponentVersion},
    machine::builder::{ComponentBuilder, SchedulerParticipation},
    memory::{Address, AddressSpace, AddressSpaceCache, AddressSpaceId},
    platform::Platform,
    scheduler::{Frequency, Period, SynchronizationContext},
};
use instruction::Mos6502InstructionSet;
use serde::{Deserialize, Serialize};

use crate::{
    cycle::{
        AddToPointerLikeRegisterSource, ArithmeticOperandInterpretation, BusMode, Cycle, Flag,
        GeneralPurposeRegister, IncrementOperand, MoveDestination, MoveSource, Phi1, Phi2,
        PointerLikeRegister, SetAddressBusSource, ShiftDirection,
    },
    decoder::{
        InstructionGroup, decode_group1_space_instruction, decode_group2_space_instruction,
        decode_group3_space_instruction, decode_undocumented_space_instruction,
    },
};

mod cycle;
mod decoder;
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
        self.0.store(value, Ordering::Release)
    }
}

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
    address_space: Arc<AddressSpace>,
    address_space_cache: AddressSpaceCache,
    timestamp: Period,
    period: Period,
}

impl Debug for Mos6502 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Mos6502")
            .field("a", &self.a)
            .field("x", &self.x)
            .field("y", &self.y)
            .field("flags", &self.flags)
            .field("stack", &self.stack)
            .field("instruction_pointer", &self.instruction_pointer)
            .field("instruction_queue", &self.instruction_queue)
            .field("bus", &self.bus)
            .field("effective_address", &self.effective_address)
            .field("operand", &self.operand)
            .field("rdy", &self.rdy)
            .finish()
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
}

impl Component for Mos6502 {
    fn snapshot_version(&self) -> Option<ComponentVersion> {
        Some(0)
    }

    fn store_snapshot(&self, mut writer: Box<dyn Write>) -> Result<(), Box<dyn std::error::Error>> {
        todo!()
    }

    fn load_snapshot(
        &mut self,
        version: ComponentVersion,
        reader: Box<dyn Read>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        match version {
            0 => {
                todo!()
            }
            other => Err(format!("Unsupported snapshot version: {other}").into()),
        }
    }

    fn synchronize(&mut self, mut context: SynchronizationContext) {
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
                            ])
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
                    self.bus.data = self
                        .address_space
                        .read_le_value(
                            self.bus.address as Address,
                            self.timestamp,
                            Some(&mut self.address_space_cache),
                        )
                        .unwrap_or_default();

                    true
                }
                BusMode::Write => false,
            };

            if self.rdy.load() || !is_read_cycle {
                for step in current_cycle.phi2.clone() {
                    match step {
                        Phi2::AddToPointerLikeRegister {
                            insert_adjustment_cycle_upon_carry: insert_carry_cycle,
                            interpretation,
                            source,
                            destination,
                        } => {
                            let mut carry = 0;

                            let value = match source {
                                AddToPointerLikeRegisterSource::Register(register) => {
                                    match register {
                                        GeneralPurposeRegister::A => self.a,
                                        GeneralPurposeRegister::X => self.x,
                                        GeneralPurposeRegister::Y => self.y,
                                    }
                                }
                                AddToPointerLikeRegisterSource::Constant(value) => value,
                                AddToPointerLikeRegisterSource::Operand => self.operand,
                            };

                            let address = match destination {
                                PointerLikeRegister::AddressBus => self.bus.address,
                                PointerLikeRegister::EffectiveAddress => {
                                    match self.effective_address.len() {
                                        1 => u16::from(self.effective_address[0]),
                                        2 => u16::from_le_bytes([
                                            self.effective_address[0],
                                            self.effective_address[1],
                                        ]),
                                        _ => unreachable!(),
                                    }
                                }
                                PointerLikeRegister::InstructionPointer => self.instruction_pointer,
                            };

                            let [_, address_high] = address.to_le_bytes();

                            let result = match interpretation {
                                ArithmeticOperandInterpretation::Unsigned => {
                                    let result = address.wrapping_add(value.into());
                                    let [_, result_high] = result.to_le_bytes();

                                    if result_high != address_high {
                                        carry = 1;
                                    }

                                    result
                                }
                                ArithmeticOperandInterpretation::Signed => {
                                    let value = (value as i8) as i16;
                                    let result = address.wrapping_add_signed(value);
                                    let [_, result_high] = result.to_le_bytes();

                                    if result_high != address_high {
                                        carry = if value.is_negative() { -1 } else { 1 };
                                    }

                                    result
                                }
                            };

                            let [result_low, _] = result.to_le_bytes();

                            // These write the OLD high so that the extra cycle can fix if carry arises and it must be handled

                            match destination {
                                PointerLikeRegister::AddressBus => {
                                    self.bus.address =
                                        u16::from_le_bytes([result_low, address_high])
                                }
                                PointerLikeRegister::InstructionPointer => {
                                    self.instruction_pointer =
                                        u16::from_le_bytes([result_low, address_high])
                                }
                                PointerLikeRegister::EffectiveAddress => {
                                    match self.effective_address.len() {
                                        1 => {
                                            self.effective_address[0] = result_low;
                                        }
                                        2 => {
                                            self.effective_address[0] = result_low;
                                            self.effective_address[1] = address_high;
                                        }
                                        _ => {
                                            unreachable!()
                                        }
                                    }
                                }
                            }

                            if carry != 0 && insert_carry_cycle {
                                self.instruction_queue.push_front(Cycle::new(
                                    BusMode::Read,
                                    None,
                                    [Phi2::AddCarryToPointerLikeRegister {
                                        register: destination,
                                        carry,
                                    }],
                                ));
                            }
                        }
                        Phi2::AddCarryToPointerLikeRegister { register, carry } => {
                            let address = match register {
                                PointerLikeRegister::AddressBus => self.bus.address,
                                PointerLikeRegister::InstructionPointer => self.instruction_pointer,
                                PointerLikeRegister::EffectiveAddress => {
                                    match self.effective_address.len() {
                                        // It would be impossible for a "1" to be here
                                        2 => u16::from_le_bytes([
                                            self.effective_address[0],
                                            self.effective_address[1],
                                        ]),
                                        _ => unreachable!(),
                                    }
                                }
                            };

                            let [address_low, address_high] = address.to_le_bytes();
                            let result = address_high.wrapping_add_signed(carry);

                            match register {
                                PointerLikeRegister::AddressBus => {
                                    self.bus.address = u16::from_le_bytes([address_low, result])
                                }
                                PointerLikeRegister::EffectiveAddress => {
                                    self.effective_address[0] = address_low;
                                    self.effective_address[1] = result;
                                }
                                PointerLikeRegister::InstructionPointer => {
                                    self.instruction_pointer =
                                        u16::from_le_bytes([address_low, result])
                                }
                            }
                        }
                        Phi2::Move {
                            source,
                            destination,
                        } => {
                            let value = match source {
                                MoveSource::Register { register } => match register {
                                    GeneralPurposeRegister::A => self.a,
                                    GeneralPurposeRegister::X => self.x,
                                    GeneralPurposeRegister::Y => self.y,
                                },
                                MoveSource::Operand => self.operand,
                                MoveSource::Stack => self.stack,
                                MoveSource::Data => self.bus.data,
                                MoveSource::Constant(value) => value,
                                MoveSource::Flags { break_ } => self.flags.to_byte(break_),
                                MoveSource::InstructionPointer { offset } => {
                                    self.instruction_pointer.to_le_bytes()[offset as usize]
                                }
                            };

                            match destination {
                                MoveDestination::Register {
                                    register,
                                    update_nz,
                                } => {
                                    if update_nz {
                                        self.flags.negative = (value as i8).is_negative();
                                        self.flags.zero = value == 0;
                                    }

                                    match register {
                                        GeneralPurposeRegister::A => self.a = value,
                                        GeneralPurposeRegister::X => self.x = value,
                                        GeneralPurposeRegister::Y => self.y = value,
                                    }
                                }
                                MoveDestination::Operand => self.operand = value,
                                MoveDestination::Stack => self.stack = value,
                                MoveDestination::EffectiveAddress => {
                                    self.effective_address.push(value);
                                }
                                MoveDestination::Opcode => {
                                    let instruction_identifier =
                                        InstructionGroup::from_repr(self.bus.data & 0b11).unwrap();
                                    let secondary_instruction_identifier =
                                        (self.bus.data >> 5) & 0b111;
                                    let argument = (self.bus.data >> 2) & 0b111;

                                    let (opcode, addressing_mode) = match instruction_identifier {
                                        InstructionGroup::Group3 => {
                                            decode_group3_space_instruction(
                                                secondary_instruction_identifier,
                                                argument,
                                                self.config.kind,
                                            )
                                        }
                                        InstructionGroup::Group1 => {
                                            decode_group1_space_instruction(
                                                secondary_instruction_identifier,
                                                argument,
                                                self.config.kind,
                                            )
                                        }
                                        InstructionGroup::Group2 => {
                                            decode_group2_space_instruction(
                                                secondary_instruction_identifier,
                                                argument,
                                                self.config.kind,
                                            )
                                        }
                                        InstructionGroup::Undocumented => {
                                            decode_undocumented_space_instruction(
                                                secondary_instruction_identifier,
                                                argument,
                                                self.config.kind,
                                            )
                                        }
                                    };

                                    let instruction = Mos6502InstructionSet {
                                        opcode,
                                        addressing_mode,
                                    };

                                    assert!(
                                        instruction.addressing_mode.is_none_or(|addressing_mode| {
                                            addressing_mode.is_valid_for_mode(self.config.kind)
                                        }),
                                        "Invalid addressing mode for instruction for mode {:?}: {:?}",
                                        self.config.kind,
                                        instruction,
                                    );

                                    tracing::debug!(
                                        "Decoded instruction {:?} at address {:x}",
                                        instruction,
                                        self.instruction_pointer.wrapping_sub(1)
                                    );

                                    self.push_steps_for_instruction(&instruction);
                                }
                                MoveDestination::Data => {
                                    self.bus.data = value;
                                }
                                MoveDestination::Flags => {
                                    self.flags = FlagRegister::from_byte(value);
                                }
                            };
                        }
                        Phi2::SetFlag { flag, value } => match flag {
                            Flag::Carry => self.flags.carry = value,
                            Flag::Zero => self.flags.zero = value,
                            Flag::Overflow => self.flags.overflow = value,
                            Flag::Negative => self.flags.negative = value,
                            Flag::Decimal => self.flags.decimal = value,
                            Flag::InterruptDisable => self.flags.interrupt_disable = value,
                        },
                        Phi2::LoadInstructionPointerFromEffectiveAddress => {
                            match self.effective_address.len() {
                                1 => {
                                    self.instruction_pointer = u16::from(self.effective_address[0]);
                                }
                                2 => {
                                    self.instruction_pointer = u16::from_le_bytes([
                                        self.effective_address[0],
                                        self.effective_address[1],
                                    ])
                                }
                                _ => unreachable!(),
                            }

                            self.effective_address.clear();
                        }
                        Phi2::Increment { operand, subtract } => {
                            let operand = match operand {
                                IncrementOperand::X => &mut self.x,
                                IncrementOperand::Y => &mut self.y,
                                IncrementOperand::Operand => &mut self.operand,
                            };

                            let delta: i8 = if subtract { -1 } else { 1 };

                            *operand = operand.wrapping_add_signed(delta);

                            self.flags.negative = (*operand as i8).is_negative();
                            self.flags.zero = *operand == 0;
                        }
                        Phi2::Compare { register } => {
                            let value = match register {
                                GeneralPurposeRegister::A => self.a,
                                GeneralPurposeRegister::X => self.x,
                                GeneralPurposeRegister::Y => self.y,
                            };

                            let (result, carry) = value.overflowing_sub(self.operand);

                            self.flags.carry = !carry;
                            self.flags.zero = result == 0;
                            self.flags.negative = (result as i8).is_negative();
                        }
                        Phi2::IncrementStack { subtract } => {
                            self.stack = if subtract {
                                self.stack.wrapping_sub(1)
                            } else {
                                self.stack.wrapping_add(1)
                            };
                        }
                        Phi2::IncrementInstructionPointer => {
                            self.instruction_pointer = self.instruction_pointer.wrapping_add(1);
                        }
                        Phi2::And { writeback } => {
                            let result = self.a & self.operand;

                            self.flags.zero = result == 0;

                            if writeback {
                                self.a = result;

                                self.flags.negative = (result as i8).is_negative();
                            } else {
                                self.flags.negative = (self.operand as i8).is_negative();
                                self.flags.overflow = (self.operand & 0b0100_0000) != 0;
                            };
                        }
                        Phi2::Or => {
                            let result = self.a | self.operand;

                            self.flags.zero = result == 0;
                            self.flags.negative = (result as i8).is_negative();

                            self.a = result;
                        }
                        Phi2::Xor => {
                            let result = self.a ^ self.operand;

                            self.flags.zero = result == 0;
                            self.flags.negative = (result as i8).is_negative();

                            self.a = result;
                        }
                        Phi2::Shift {
                            direction,
                            rotate,
                            a_is_operand,
                        } => {
                            let operand = if a_is_operand {
                                &mut self.a
                            } else {
                                &mut self.operand
                            };

                            let shift_input = if rotate { self.flags.carry } else { false };

                            match direction {
                                ShiftDirection::Left => {
                                    let shift_output = (*operand & 0b1000_0000) != 0;
                                    self.flags.carry = shift_output;

                                    *operand = (*operand << 1) | (shift_input as u8);
                                }
                                ShiftDirection::Right => {
                                    let shift_output = (*operand & 0b0000_0001) != 0;
                                    self.flags.carry = shift_output;

                                    *operand = (*operand >> 1) | ((shift_input as u8) << 7);
                                }
                            }

                            self.flags.zero = *operand == 0;
                            self.flags.negative = (*operand as i8).is_negative();
                        }
                        Phi2::Add { invert_operand } => {
                            let operand = if invert_operand {
                                !self.operand
                            } else {
                                self.operand
                            };

                            let (first_operation_result, first_operation_carry) =
                                self.a.overflowing_add(operand);

                            let (second_operation_result, second_operation_carry) =
                                first_operation_result.overflowing_add(self.flags.carry as u8);

                            self.flags.overflow = ((self.a & 0b1000_0000)
                                == (operand & 0b1000_0000))
                                && ((self.a & 0b1000_0000)
                                    != (second_operation_result & 0b1000_0000));

                            self.flags.carry = first_operation_carry || second_operation_carry;

                            self.flags.negative = (second_operation_result as i8).is_negative();

                            self.flags.zero = second_operation_result == 0;

                            self.a = second_operation_result;
                        }
                    }
                }

                match current_cycle.bus_mode {
                    BusMode::Read => {}
                    BusMode::Write => {
                        self.address_space
                            .write_le_value(
                                self.bus.address as Address,
                                self.timestamp,
                                Some(&mut self.address_space_cache),
                                self.bus.data,
                            )
                            .unwrap_or_default();
                    }
                }

                tracing::debug!("Current cycle {:x?}, State {:x?}", current_cycle, self);

                // Check for interrupts

                if self.config.kind.supports_interrupts() && self.instruction_queue.is_empty() {
                    if self.nmi.interrupt_required() {
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

    fn build_component(
        self,
        component_builder: ComponentBuilder<'_, P, Self::Component>,
    ) -> Result<Self::Component, Box<dyn std::error::Error>> {
        let address_space = component_builder
            .get_address_space(self.assigned_address_space)
            .clone();

        component_builder.set_scheduler_participation(SchedulerParticipation::SchedulerDriven);

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
            address_space_cache: address_space.cache(),
            address_space,
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
