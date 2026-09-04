use core::fmt::Display;

use serde::{Deserialize, Serialize};

use crate::{
    IRQ_VECTOR,
    component::Mos6502,
    cycle::{
        AddToPointerLikeRegisterSource, ArithmeticOperandInterpretation, BusMode, Cycle, Flag,
        GeneralPurposeRegister, IncrementOperand, IndexAdjustment, MoveDestination, MoveSource,
        Phi1Source, Phi2, PointerLikeRegister, ShiftDirection, UnstableStoreSource,
    },
    variant::Variant,
};

// https://www.pagetable.com/c64ref/6502/?tab=2

#[derive(Debug, Copy, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Mos6502AddressingMode {
    Immediate,
    Absolute,
    XIndexedAbsolute,
    YIndexedAbsolute,
    AbsoluteIndirect,
    ZeroPage,
    XIndexedZeroPage,
    YIndexedZeroPage,
    XIndexedZeroPageIndirect,
    ZeroPageIndirectYIndexed,
    Relative,
    Accumulator,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Wdc65C02AddressingMode {
    ZeroPageIndirect,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum AddressingMode {
    Mos6502(Mos6502AddressingMode),
    Wdc65C02(Wdc65C02AddressingMode),
}

#[derive(
    Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, strum::Display,
)]
pub enum Mos6502Opcode {
    Adc,
    Anc,
    And,
    Arr,
    Asl,
    Asr,
    Bcc,
    Bcs,
    Beq,
    Bit,
    Bmi,
    Bne,
    Bpl,
    Brk,
    Bvc,
    Bvs,
    Clc,
    Cld,
    Cli,
    Clv,
    Cmp,
    Cpx,
    Cpy,
    Dcp,
    Dec,
    Dex,
    Dey,
    Eor,
    Inc,
    Inx,
    Iny,
    Isc,
    Jam,
    Jmp,
    Jsr,
    Las,
    Lax,
    Lda,
    Ldx,
    Ldy,
    Lsr,
    Nop,
    Ora,
    Pha,
    Php,
    Pla,
    Plp,
    Rla,
    Rol,
    Ror,
    Rra,
    Rti,
    Rts,
    Sax,
    Sbc,
    Sbx,
    Sec,
    Sed,
    Sei,
    Sha,
    Shs,
    Shx,
    Shy,
    Slo,
    Sre,
    Sta,
    Stx,
    Sty,
    Tax,
    Tay,
    Tsx,
    Txa,
    Txs,
    Tya,
    Xaa,
}

#[derive(
    Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, strum::Display,
)]
pub enum Wdc65C02Opcode {
    Bra,
    Phx,
    Phy,
    Plx,
    Ply,
    Stz,
    Trb,
    Tsb,
    // Apparently these two only exist on some 65C02Os but for simplicity sake we will treat all
    // of them as having these two
    Stp,
    Wai,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Opcode {
    Mos6502(Mos6502Opcode),
    Wdc65C02(Wdc65C02Opcode),
}

impl Display for Opcode {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Opcode::Mos6502(opcode) => write!(f, "{opcode}"),
            Opcode::Wdc65C02(opcode) => write!(f, "{opcode}"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Mos6502InstructionSet {
    pub opcode: Opcode,
    pub addressing_mode: Option<AddressingMode>,
}

impl<V: Variant> Mos6502<V> {
    pub(super) fn push_steps_for_instruction(&mut self, instruction: &Mos6502InstructionSet) {
        let index_adjustment = Self::index_adjustment(instruction.opcode);

        if let Some(addressing_mode) = instruction.addressing_mode {
            match addressing_mode {
                AddressingMode::Mos6502(Mos6502AddressingMode::Absolute) => {
                    self.state.cycle_queue.extend([
                        Cycle::new(
                            BusMode::Read,
                            Some(Phi1Source::InstructionPointer),
                            [
                                Phi2::IncrementInstructionPointer,
                                Phi2::Move {
                                    source: MoveSource::Data,
                                    destination: MoveDestination::EffectiveAddress,
                                },
                            ],
                        ),
                        Cycle::new(
                            BusMode::Read,
                            Some(Phi1Source::InstructionPointer),
                            [
                                Phi2::IncrementInstructionPointer,
                                Phi2::Move {
                                    source: MoveSource::Data,
                                    destination: MoveDestination::EffectiveAddress,
                                },
                            ],
                        ),
                    ]);
                }
                AddressingMode::Mos6502(
                    Mos6502AddressingMode::Immediate | Mos6502AddressingMode::Relative,
                ) => {
                    self.state.cycle_queue.extend([Cycle::new(
                        BusMode::Read,
                        Some(Phi1Source::InstructionPointer),
                        [Phi2::IncrementInstructionPointer],
                    )]);
                }
                AddressingMode::Mos6502(Mos6502AddressingMode::XIndexedAbsolute) => {
                    self.register_indexed_absolute(GeneralPurposeRegister::X, index_adjustment);
                }
                AddressingMode::Mos6502(Mos6502AddressingMode::YIndexedAbsolute) => {
                    self.register_indexed_absolute(GeneralPurposeRegister::Y, index_adjustment);
                }
                AddressingMode::Mos6502(Mos6502AddressingMode::AbsoluteIndirect) => {
                    self.state.cycle_queue.extend([
                        Cycle::new(
                            BusMode::Read,
                            Some(Phi1Source::InstructionPointer),
                            [
                                Phi2::IncrementInstructionPointer,
                                Phi2::Move {
                                    source: MoveSource::Data,
                                    destination: MoveDestination::EffectiveAddress,
                                },
                            ],
                        ),
                        Cycle::new(
                            BusMode::Read,
                            Some(Phi1Source::InstructionPointer),
                            [
                                Phi2::IncrementInstructionPointer,
                                Phi2::Move {
                                    source: MoveSource::Data,
                                    destination: MoveDestination::EffectiveAddress,
                                },
                            ],
                        ),
                        Cycle::new(
                            BusMode::Read,
                            Some(Phi1Source::EffectiveAddress),
                            [
                                Phi2::Move {
                                    source: MoveSource::Data,
                                    destination: MoveDestination::EffectiveAddress,
                                },
                                Phi2::AddToPointerLikeRegister {
                                    source: AddToPointerLikeRegisterSource::Constant(1),
                                    destination: PointerLikeRegister::AddressBus,
                                    interpretation: ArithmeticOperandInterpretation::Unsigned,
                                    // Insert carry cycle if the bug is not present
                                    adjustment: if V::HAS_ABSOLUTE_INDIRECT_PAGE_WRAP_ERRATA {
                                        IndexAdjustment::Discard
                                    } else {
                                        IndexAdjustment::OnCarry
                                    },
                                },
                            ],
                        ),
                        Cycle::new(
                            BusMode::Read,
                            None,
                            [Phi2::Move {
                                source: MoveSource::Data,
                                destination: MoveDestination::EffectiveAddress,
                            }],
                        ),
                    ]);
                }
                AddressingMode::Mos6502(Mos6502AddressingMode::XIndexedZeroPageIndirect) => {
                    self.state.cycle_queue.extend([
                        Cycle::new(
                            BusMode::Read,
                            Some(Phi1Source::InstructionPointer),
                            [
                                Phi2::IncrementInstructionPointer,
                                Phi2::Move {
                                    source: MoveSource::Data,
                                    destination: MoveDestination::EffectiveAddress,
                                },
                            ],
                        ),
                        Cycle::new(
                            BusMode::Read,
                            Some(Phi1Source::EffectiveAddress),
                            [Phi2::AddToPointerLikeRegister {
                                source: AddToPointerLikeRegisterSource::Register(
                                    GeneralPurposeRegister::X,
                                ),
                                destination: PointerLikeRegister::AddressBus,
                                adjustment: IndexAdjustment::Discard,
                                interpretation: ArithmeticOperandInterpretation::Unsigned,
                            }],
                        ),
                        Cycle::new(
                            BusMode::Read,
                            None,
                            [
                                Phi2::Move {
                                    source: MoveSource::Data,
                                    destination: MoveDestination::EffectiveAddress,
                                },
                                Phi2::AddToPointerLikeRegister {
                                    source: AddToPointerLikeRegisterSource::Constant(1),
                                    destination: PointerLikeRegister::AddressBus,
                                    interpretation: ArithmeticOperandInterpretation::Unsigned,
                                    adjustment: IndexAdjustment::Discard,
                                },
                            ],
                        ),
                        Cycle::new(
                            BusMode::Read,
                            None,
                            [Phi2::Move {
                                source: MoveSource::Data,
                                destination: MoveDestination::EffectiveAddress,
                            }],
                        ),
                    ]);
                }
                AddressingMode::Mos6502(Mos6502AddressingMode::ZeroPageIndirectYIndexed) => {
                    self.state.cycle_queue.extend([
                        Cycle::new(
                            BusMode::Read,
                            Some(Phi1Source::InstructionPointer),
                            [
                                Phi2::IncrementInstructionPointer,
                                Phi2::Move {
                                    source: MoveSource::Data,
                                    destination: MoveDestination::EffectiveAddress,
                                },
                            ],
                        ),
                        Cycle::new(
                            BusMode::Read,
                            Some(Phi1Source::EffectiveAddress),
                            [
                                Phi2::Move {
                                    source: MoveSource::Data,
                                    destination: MoveDestination::EffectiveAddress,
                                },
                                Phi2::AddToPointerLikeRegister {
                                    source: AddToPointerLikeRegisterSource::Constant(1),
                                    destination: PointerLikeRegister::AddressBus,
                                    interpretation: ArithmeticOperandInterpretation::Unsigned,
                                    adjustment: IndexAdjustment::Discard,
                                },
                            ],
                        ),
                        Cycle::new(
                            BusMode::Read,
                            None,
                            [
                                Phi2::Move {
                                    source: MoveSource::Data,
                                    destination: MoveDestination::EffectiveAddress,
                                },
                                Phi2::AddToPointerLikeRegister {
                                    source: AddToPointerLikeRegisterSource::Register(
                                        GeneralPurposeRegister::Y,
                                    ),
                                    destination: PointerLikeRegister::EffectiveAddress,
                                    interpretation: ArithmeticOperandInterpretation::Unsigned,
                                    adjustment: index_adjustment,
                                },
                            ],
                        ),
                    ]);
                }
                AddressingMode::Mos6502(Mos6502AddressingMode::XIndexedZeroPage) => {
                    self.register_indexed_zero_page(GeneralPurposeRegister::X);
                }
                AddressingMode::Mos6502(Mos6502AddressingMode::YIndexedZeroPage) => {
                    self.register_indexed_zero_page(GeneralPurposeRegister::Y);
                }
                AddressingMode::Mos6502(Mos6502AddressingMode::ZeroPage) => {
                    self.state.cycle_queue.extend([Cycle::new(
                        BusMode::Read,
                        Some(Phi1Source::InstructionPointer),
                        [
                            Phi2::IncrementInstructionPointer,
                            Phi2::Move {
                                source: MoveSource::Data,
                                destination: MoveDestination::EffectiveAddress,
                            },
                        ],
                    )]);
                }
                AddressingMode::Mos6502(Mos6502AddressingMode::Accumulator) => {
                    self.state.cycle_queue.extend([Cycle::dummy()]);
                }
                AddressingMode::Wdc65C02(Wdc65C02AddressingMode::ZeroPageIndirect) => {
                    todo!()
                }
            }
        } else {
            self.state.cycle_queue.extend([Cycle::dummy()]);
        }

        match instruction.opcode {
            Opcode::Mos6502(Mos6502Opcode::Adc) => {
                self.patch_read_maybe_effective_address_dependent(
                    instruction,
                    [
                        Phi2::Move {
                            source: MoveSource::Data,
                            destination: MoveDestination::Operand,
                        },
                        Phi2::Add {
                            invert_operand: false,
                        },
                    ],
                );
            }
            Opcode::Mos6502(Mos6502Opcode::Anc) => {
                self.patch_read_maybe_effective_address_dependent(
                    instruction,
                    [
                        Phi2::Move {
                            source: MoveSource::Data,
                            destination: MoveDestination::Operand,
                        },
                        Phi2::And { writeback: true },
                        Phi2::CopyFlag {
                            source: Flag::Negative,
                            destination: Flag::Carry,
                        },
                    ],
                );
            }
            Opcode::Mos6502(Mos6502Opcode::And) => {
                self.patch_read_maybe_effective_address_dependent(
                    instruction,
                    [
                        Phi2::Move {
                            source: MoveSource::Data,
                            destination: MoveDestination::Operand,
                        },
                        Phi2::And { writeback: true },
                    ],
                );
            }
            Opcode::Mos6502(Mos6502Opcode::Arr) => {
                self.patch_read_maybe_effective_address_dependent(
                    instruction,
                    [
                        Phi2::Move {
                            source: MoveSource::Data,
                            destination: MoveDestination::Operand,
                        },
                        Phi2::And { writeback: true },
                        Phi2::RotateRightThroughAdder,
                    ],
                );
            }
            Opcode::Mos6502(Mos6502Opcode::Asl) => {
                if instruction.addressing_mode
                    == Some(AddressingMode::Mos6502(Mos6502AddressingMode::Accumulator))
                {
                    self.patch_read_maybe_effective_address_dependent(
                        instruction,
                        [Phi2::Shift {
                            direction: ShiftDirection::Left,
                            rotate: false,
                            a_is_operand: true,
                        }],
                    );
                } else {
                    self.insert_rmw_effective_address_dependent([Phi2::Shift {
                        direction: ShiftDirection::Left,
                        rotate: false,
                        a_is_operand: false,
                    }]);
                }
            }
            Opcode::Mos6502(Mos6502Opcode::Asr) => {
                self.patch_read_maybe_effective_address_dependent(
                    instruction,
                    [
                        Phi2::Move {
                            source: MoveSource::Data,
                            destination: MoveDestination::Operand,
                        },
                        Phi2::And { writeback: true },
                        Phi2::Shift {
                            direction: ShiftDirection::Right,
                            rotate: false,
                            a_is_operand: true,
                        },
                    ],
                );
            }
            Opcode::Mos6502(Mos6502Opcode::Bit) => {
                self.patch_read_maybe_effective_address_dependent(
                    instruction,
                    [
                        Phi2::Move {
                            source: MoveSource::Data,
                            destination: MoveDestination::Operand,
                        },
                        Phi2::And { writeback: false },
                    ],
                );
            }
            Opcode::Mos6502(Mos6502Opcode::Brk) => {
                tracing::debug!("BRK occurred");

                self.state.cycle_queue.clear();

                self.state.cycle_queue.extend([
                    Cycle::new(
                        BusMode::Read,
                        Some(Phi1Source::InstructionPointer),
                        [Phi2::IncrementInstructionPointer],
                    ),
                    Cycle::new(
                        BusMode::Write,
                        Some(Phi1Source::Stack),
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
                        Some(Phi1Source::Stack),
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
                        Some(Phi1Source::Stack),
                        [
                            Phi2::Move {
                                source: MoveSource::Flags { break_: true },
                                destination: MoveDestination::Data,
                            },
                            Phi2::IncrementStack { subtract: true },
                        ],
                    ),
                    Cycle::new(
                        BusMode::Read,
                        Some(Phi1Source::Constant(IRQ_VECTOR)),
                        [Phi2::Move {
                            source: MoveSource::Data,
                            destination: MoveDestination::EffectiveAddress,
                        }],
                    ),
                    Cycle::new(
                        BusMode::Read,
                        Some(Phi1Source::Constant(IRQ_VECTOR + 1)),
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
            Opcode::Mos6502(Mos6502Opcode::Clc) => {
                self.patch_read_maybe_effective_address_dependent(
                    instruction,
                    [Phi2::SetFlag {
                        flag: Flag::Carry,
                        value: false,
                    }],
                );
            }
            Opcode::Mos6502(Mos6502Opcode::Cld) => {
                self.patch_read_maybe_effective_address_dependent(
                    instruction,
                    [Phi2::SetFlag {
                        flag: Flag::Decimal,
                        value: false,
                    }],
                );
            }
            Opcode::Mos6502(Mos6502Opcode::Cli) => {
                self.patch_read_maybe_effective_address_dependent(
                    instruction,
                    [Phi2::SetFlag {
                        flag: Flag::InterruptDisable,
                        value: false,
                    }],
                );
            }
            Opcode::Mos6502(Mos6502Opcode::Clv) => {
                self.patch_read_maybe_effective_address_dependent(
                    instruction,
                    [Phi2::SetFlag {
                        flag: Flag::Overflow,
                        value: false,
                    }],
                );
            }
            Opcode::Mos6502(Mos6502Opcode::Cmp) => {
                self.patch_read_maybe_effective_address_dependent(
                    instruction,
                    [
                        Phi2::Move {
                            source: MoveSource::Data,
                            destination: MoveDestination::Operand,
                        },
                        Phi2::Compare {
                            register: GeneralPurposeRegister::A,
                        },
                    ],
                );
            }
            Opcode::Mos6502(Mos6502Opcode::Cpx) => {
                self.patch_read_maybe_effective_address_dependent(
                    instruction,
                    [
                        Phi2::Move {
                            source: MoveSource::Data,
                            destination: MoveDestination::Operand,
                        },
                        Phi2::Compare {
                            register: GeneralPurposeRegister::X,
                        },
                    ],
                );
            }
            Opcode::Mos6502(Mos6502Opcode::Cpy) => {
                self.patch_read_maybe_effective_address_dependent(
                    instruction,
                    [
                        Phi2::Move {
                            source: MoveSource::Data,
                            destination: MoveDestination::Operand,
                        },
                        Phi2::Compare {
                            register: GeneralPurposeRegister::Y,
                        },
                    ],
                );
            }
            Opcode::Mos6502(Mos6502Opcode::Dcp) => {
                self.insert_rmw_effective_address_dependent([
                    Phi2::Increment {
                        operand: IncrementOperand::Operand,
                        subtract: true,
                    },
                    Phi2::Compare {
                        register: GeneralPurposeRegister::A,
                    },
                ]);
            }
            Opcode::Mos6502(Mos6502Opcode::Dec) => {
                self.insert_rmw_effective_address_dependent([Phi2::Increment {
                    operand: IncrementOperand::Operand,
                    subtract: true,
                }]);
            }
            Opcode::Mos6502(Mos6502Opcode::Dex) => {
                self.patch_read_maybe_effective_address_dependent(
                    instruction,
                    [Phi2::Increment {
                        operand: IncrementOperand::X,
                        subtract: true,
                    }],
                );
            }
            Opcode::Mos6502(Mos6502Opcode::Dey) => {
                self.patch_read_maybe_effective_address_dependent(
                    instruction,
                    [Phi2::Increment {
                        operand: IncrementOperand::Y,
                        subtract: true,
                    }],
                );
            }
            Opcode::Mos6502(Mos6502Opcode::Eor) => {
                self.patch_read_maybe_effective_address_dependent(
                    instruction,
                    [
                        Phi2::Move {
                            source: MoveSource::Data,
                            destination: MoveDestination::Operand,
                        },
                        Phi2::Xor,
                    ],
                );
            }
            Opcode::Mos6502(Mos6502Opcode::Inc) => {
                self.insert_rmw_effective_address_dependent([Phi2::Increment {
                    operand: IncrementOperand::Operand,
                    subtract: false,
                }]);
            }
            Opcode::Mos6502(Mos6502Opcode::Inx) => {
                self.patch_read_maybe_effective_address_dependent(
                    instruction,
                    [Phi2::Increment {
                        operand: IncrementOperand::X,
                        subtract: false,
                    }],
                );
            }
            Opcode::Mos6502(Mos6502Opcode::Iny) => {
                self.patch_read_maybe_effective_address_dependent(
                    instruction,
                    [Phi2::Increment {
                        operand: IncrementOperand::Y,
                        subtract: false,
                    }],
                );
            }
            Opcode::Mos6502(Mos6502Opcode::Isc) => {
                self.insert_rmw_effective_address_dependent([
                    Phi2::Increment {
                        operand: IncrementOperand::Operand,
                        subtract: false,
                    },
                    Phi2::Add {
                        invert_operand: true,
                    },
                ]);
            }
            Opcode::Mos6502(Mos6502Opcode::Jam) => {
                tracing::error!("JAM occurred");

                self.state.cycle_queue.clear();
                self.state
                    .cycle_queue
                    .push_back(Cycle::new(
                        BusMode::Read,
                        Some(Phi1Source::Constant(u16::MAX)),
                        [Phi2::Jam],
                    ))
                    .unwrap();
            }
            Opcode::Mos6502(Mos6502Opcode::Jmp) => {
                // Note that this is correct for all actual existing addressing modes for JMP
                self.state
                    .cycle_queue
                    .iter_mut()
                    .last()
                    .unwrap()
                    .phi2
                    .push(Phi2::LoadInstructionPointerFromEffectiveAddress)
                    .unwrap();
            }
            Opcode::Mos6502(Mos6502Opcode::Jsr) => {
                self.state.cycle_queue.clear();

                self.state.cycle_queue.extend([
                    Cycle::new(
                        BusMode::Read,
                        Some(Phi1Source::InstructionPointer),
                        [
                            Phi2::IncrementInstructionPointer,
                            Phi2::Move {
                                source: MoveSource::Data,
                                destination: MoveDestination::EffectiveAddress,
                            },
                        ],
                    ),
                    Cycle::new(
                        BusMode::Write,
                        Some(Phi1Source::Stack),
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
                        Some(Phi1Source::Stack),
                        [
                            Phi2::Move {
                                source: MoveSource::InstructionPointer { offset: 0 },
                                destination: MoveDestination::Data,
                            },
                            Phi2::IncrementStack { subtract: true },
                        ],
                    ),
                    Cycle::new(
                        BusMode::Read,
                        Some(Phi1Source::InstructionPointer),
                        [
                            Phi2::IncrementInstructionPointer,
                            Phi2::Move {
                                source: MoveSource::Data,
                                destination: MoveDestination::EffectiveAddress,
                            },
                        ],
                    ),
                    Cycle::new(
                        BusMode::Read,
                        None,
                        [Phi2::LoadInstructionPointerFromEffectiveAddress],
                    ),
                ]);
            }
            Opcode::Mos6502(Mos6502Opcode::Las) => {
                self.patch_read_maybe_effective_address_dependent(
                    instruction,
                    [
                        Phi2::Move {
                            source: MoveSource::Data,
                            destination: MoveDestination::Operand,
                        },
                        Phi2::AndOperandWithStackPointer,
                    ],
                );
            }
            Opcode::Mos6502(Mos6502Opcode::Lax) => {
                self.patch_read_maybe_effective_address_dependent(
                    instruction,
                    [
                        Phi2::Move {
                            source: MoveSource::Data,
                            destination: MoveDestination::Register {
                                register: GeneralPurposeRegister::A,
                                update_nz: true,
                            },
                        },
                        Phi2::Move {
                            source: MoveSource::Data,
                            destination: MoveDestination::Register {
                                register: GeneralPurposeRegister::X,
                                update_nz: true,
                            },
                        },
                    ],
                );
            }
            Opcode::Mos6502(Mos6502Opcode::Lda) => {
                self.patch_read_maybe_effective_address_dependent(
                    instruction,
                    [Phi2::Move {
                        source: MoveSource::Data,
                        destination: MoveDestination::Register {
                            register: GeneralPurposeRegister::A,
                            update_nz: true,
                        },
                    }],
                );
            }
            Opcode::Mos6502(Mos6502Opcode::Ldx) => {
                self.patch_read_maybe_effective_address_dependent(
                    instruction,
                    [Phi2::Move {
                        source: MoveSource::Data,
                        destination: MoveDestination::Register {
                            register: GeneralPurposeRegister::X,
                            update_nz: true,
                        },
                    }],
                );
            }
            Opcode::Mos6502(Mos6502Opcode::Ldy) => {
                self.patch_read_maybe_effective_address_dependent(
                    instruction,
                    [Phi2::Move {
                        source: MoveSource::Data,
                        destination: MoveDestination::Register {
                            register: GeneralPurposeRegister::Y,
                            update_nz: true,
                        },
                    }],
                );
            }
            Opcode::Mos6502(Mos6502Opcode::Lsr) => {
                if instruction.addressing_mode
                    == Some(AddressingMode::Mos6502(Mos6502AddressingMode::Accumulator))
                {
                    self.patch_read_maybe_effective_address_dependent(
                        instruction,
                        [Phi2::Shift {
                            direction: ShiftDirection::Right,
                            rotate: false,
                            a_is_operand: true,
                        }],
                    );
                } else {
                    self.insert_rmw_effective_address_dependent([Phi2::Shift {
                        direction: ShiftDirection::Right,
                        rotate: false,
                        a_is_operand: false,
                    }]);
                }
            }
            Opcode::Mos6502(Mos6502Opcode::Nop) => {
                // Handle the multibyte forms if required
                self.patch_read_maybe_effective_address_dependent(instruction, []);
            }
            Opcode::Mos6502(Mos6502Opcode::Ora) => {
                self.patch_read_maybe_effective_address_dependent(
                    instruction,
                    [
                        Phi2::Move {
                            source: MoveSource::Data,
                            destination: MoveDestination::Operand,
                        },
                        Phi2::Or,
                    ],
                );
            }
            Opcode::Mos6502(Mos6502Opcode::Pha) => {
                self.push_stack_item(MoveSource::Register {
                    register: GeneralPurposeRegister::A,
                });
            }
            Opcode::Mos6502(Mos6502Opcode::Php) => {
                self.push_stack_item(MoveSource::Flags { break_: true });
            }
            Opcode::Mos6502(Mos6502Opcode::Pla) => {
                self.pull_stack_item(MoveDestination::Register {
                    register: GeneralPurposeRegister::A,
                    update_nz: true,
                });
            }
            Opcode::Mos6502(Mos6502Opcode::Plp) => {
                self.pull_stack_item(MoveDestination::Flags);
            }
            Opcode::Mos6502(Mos6502Opcode::Rla) => {
                self.insert_rmw_effective_address_dependent([
                    Phi2::Shift {
                        direction: ShiftDirection::Left,
                        rotate: true,
                        a_is_operand: false,
                    },
                    Phi2::And { writeback: true },
                ]);
            }
            Opcode::Mos6502(Mos6502Opcode::Rol) => {
                if instruction.addressing_mode
                    == Some(AddressingMode::Mos6502(Mos6502AddressingMode::Accumulator))
                {
                    self.patch_read_maybe_effective_address_dependent(
                        instruction,
                        [Phi2::Shift {
                            direction: ShiftDirection::Left,
                            rotate: true,
                            a_is_operand: true,
                        }],
                    );
                } else {
                    self.insert_rmw_effective_address_dependent([Phi2::Shift {
                        direction: ShiftDirection::Left,
                        rotate: true,
                        a_is_operand: false,
                    }]);
                }
            }
            Opcode::Mos6502(Mos6502Opcode::Ror) => {
                if instruction.addressing_mode
                    == Some(AddressingMode::Mos6502(Mos6502AddressingMode::Accumulator))
                {
                    self.patch_read_maybe_effective_address_dependent(
                        instruction,
                        [Phi2::Shift {
                            direction: ShiftDirection::Right,
                            rotate: true,
                            a_is_operand: true,
                        }],
                    );
                } else {
                    self.insert_rmw_effective_address_dependent([Phi2::Shift {
                        direction: ShiftDirection::Right,
                        rotate: true,
                        a_is_operand: false,
                    }]);
                }
            }
            Opcode::Mos6502(Mos6502Opcode::Rra) => {
                self.insert_rmw_effective_address_dependent([
                    Phi2::Shift {
                        direction: ShiftDirection::Right,
                        rotate: true,
                        a_is_operand: false,
                    },
                    Phi2::Add {
                        invert_operand: false,
                    },
                ]);
            }
            Opcode::Mos6502(Mos6502Opcode::Rti) => {
                self.state.cycle_queue.clear();

                self.state.cycle_queue.extend([
                    Cycle::new(
                        BusMode::Read,
                        None,
                        [Phi2::IncrementStack { subtract: false }],
                    ),
                    Cycle::new(
                        BusMode::Read,
                        Some(Phi1Source::Stack),
                        [
                            Phi2::Move {
                                source: MoveSource::Data,
                                destination: MoveDestination::Flags,
                            },
                            Phi2::IncrementStack { subtract: false },
                        ],
                    ),
                    Cycle::new(
                        BusMode::Read,
                        Some(Phi1Source::Stack),
                        [
                            Phi2::Move {
                                source: MoveSource::Data,
                                destination: MoveDestination::EffectiveAddress,
                            },
                            Phi2::IncrementStack { subtract: false },
                        ],
                    ),
                    Cycle::new(
                        BusMode::Read,
                        Some(Phi1Source::Stack),
                        [Phi2::Move {
                            source: MoveSource::Data,
                            destination: MoveDestination::EffectiveAddress,
                        }],
                    ),
                    Cycle::new(
                        BusMode::Read,
                        None,
                        [Phi2::LoadInstructionPointerFromEffectiveAddress],
                    ),
                ]);
            }
            Opcode::Mos6502(Mos6502Opcode::Rts) => {
                self.state.cycle_queue.clear();

                self.state.cycle_queue.extend([
                    Cycle::new(
                        BusMode::Read,
                        None,
                        [Phi2::IncrementStack { subtract: false }],
                    ),
                    Cycle::new(
                        BusMode::Read,
                        Some(Phi1Source::Stack),
                        [
                            Phi2::Move {
                                source: MoveSource::Data,
                                destination: MoveDestination::EffectiveAddress,
                            },
                            Phi2::IncrementStack { subtract: false },
                        ],
                    ),
                    Cycle::new(
                        BusMode::Read,
                        Some(Phi1Source::Stack),
                        [Phi2::Move {
                            source: MoveSource::Data,
                            destination: MoveDestination::EffectiveAddress,
                        }],
                    ),
                    Cycle::new(
                        BusMode::Read,
                        None,
                        [Phi2::LoadInstructionPointerFromEffectiveAddress],
                    ),
                    Cycle::new(BusMode::Read, None, [Phi2::IncrementInstructionPointer]),
                ]);
            }
            Opcode::Mos6502(Mos6502Opcode::Sax) => {
                self.insert_write_effective_address_dependent(
                    instruction,
                    [Phi2::Move {
                        source: MoveSource::AccumulatorAndX,
                        destination: MoveDestination::Data,
                    }],
                );
            }
            Opcode::Mos6502(Mos6502Opcode::Sbc) => {
                self.patch_read_maybe_effective_address_dependent(
                    instruction,
                    [
                        Phi2::Move {
                            source: MoveSource::Data,
                            destination: MoveDestination::Operand,
                        },
                        Phi2::Add {
                            invert_operand: true,
                        },
                    ],
                );
            }
            Opcode::Mos6502(Mos6502Opcode::Sbx) => {
                self.patch_read_maybe_effective_address_dependent(
                    instruction,
                    [
                        Phi2::Move {
                            source: MoveSource::Data,
                            destination: MoveDestination::Operand,
                        },
                        Phi2::SubtractOperandFromAAndX,
                    ],
                );
            }
            Opcode::Mos6502(Mos6502Opcode::Sec) => {
                self.patch_read_maybe_effective_address_dependent(
                    instruction,
                    [Phi2::SetFlag {
                        flag: Flag::Carry,
                        value: true,
                    }],
                );
            }
            Opcode::Mos6502(Mos6502Opcode::Sed) => {
                self.patch_read_maybe_effective_address_dependent(
                    instruction,
                    [Phi2::SetFlag {
                        flag: Flag::Decimal,
                        value: true,
                    }],
                );
            }
            Opcode::Mos6502(Mos6502Opcode::Sei) => {
                self.patch_read_maybe_effective_address_dependent(
                    instruction,
                    [Phi2::SetFlag {
                        flag: Flag::InterruptDisable,
                        value: true,
                    }],
                );
            }
            Opcode::Mos6502(
                Mos6502Opcode::Sha | Mos6502Opcode::Shs | Mos6502Opcode::Shx | Mos6502Opcode::Shy,
            ) => {
                self.insert_write_effective_address_dependent(
                    instruction,
                    [Phi2::Move {
                        source: MoveSource::Operand,
                        destination: MoveDestination::Data,
                    }],
                );
            }
            Opcode::Mos6502(Mos6502Opcode::Slo) => {
                self.insert_rmw_effective_address_dependent([
                    Phi2::Shift {
                        direction: ShiftDirection::Left,
                        rotate: false,
                        a_is_operand: false,
                    },
                    Phi2::Or,
                ]);
            }
            Opcode::Mos6502(Mos6502Opcode::Sre) => {
                self.insert_rmw_effective_address_dependent([
                    Phi2::Shift {
                        direction: ShiftDirection::Right,
                        rotate: false,
                        a_is_operand: false,
                    },
                    Phi2::Xor,
                ]);
            }
            Opcode::Mos6502(Mos6502Opcode::Sta) => {
                self.insert_write_effective_address_dependent(
                    instruction,
                    [Phi2::Move {
                        source: MoveSource::Register {
                            register: GeneralPurposeRegister::A,
                        },
                        destination: MoveDestination::Data,
                    }],
                );
            }
            Opcode::Mos6502(Mos6502Opcode::Stx) => {
                self.insert_write_effective_address_dependent(
                    instruction,
                    [Phi2::Move {
                        source: MoveSource::Register {
                            register: GeneralPurposeRegister::X,
                        },
                        destination: MoveDestination::Data,
                    }],
                );
            }
            Opcode::Mos6502(Mos6502Opcode::Sty) => {
                self.insert_write_effective_address_dependent(
                    instruction,
                    [Phi2::Move {
                        source: MoveSource::Register {
                            register: GeneralPurposeRegister::Y,
                        },
                        destination: MoveDestination::Data,
                    }],
                );
            }
            Opcode::Mos6502(Mos6502Opcode::Tax) => {
                self.patch_read_maybe_effective_address_dependent(
                    instruction,
                    [Phi2::Move {
                        source: MoveSource::Register {
                            register: GeneralPurposeRegister::A,
                        },
                        destination: MoveDestination::Register {
                            register: GeneralPurposeRegister::X,
                            update_nz: true,
                        },
                    }],
                );
            }
            Opcode::Mos6502(Mos6502Opcode::Tay) => {
                self.patch_read_maybe_effective_address_dependent(
                    instruction,
                    [Phi2::Move {
                        source: MoveSource::Register {
                            register: GeneralPurposeRegister::A,
                        },
                        destination: MoveDestination::Register {
                            register: GeneralPurposeRegister::Y,
                            update_nz: true,
                        },
                    }],
                );
            }
            Opcode::Mos6502(Mos6502Opcode::Tsx) => {
                self.patch_read_maybe_effective_address_dependent(
                    instruction,
                    [Phi2::Move {
                        source: MoveSource::Stack,
                        destination: MoveDestination::Register {
                            register: GeneralPurposeRegister::X,
                            update_nz: true,
                        },
                    }],
                );
            }
            Opcode::Mos6502(Mos6502Opcode::Txa) => {
                self.patch_read_maybe_effective_address_dependent(
                    instruction,
                    [Phi2::Move {
                        source: MoveSource::Register {
                            register: GeneralPurposeRegister::X,
                        },
                        destination: MoveDestination::Register {
                            register: GeneralPurposeRegister::A,
                            update_nz: true,
                        },
                    }],
                );
            }
            Opcode::Mos6502(Mos6502Opcode::Txs) => {
                self.patch_read_maybe_effective_address_dependent(
                    instruction,
                    [Phi2::Move {
                        source: MoveSource::Register {
                            register: GeneralPurposeRegister::X,
                        },
                        destination: MoveDestination::Stack,
                    }],
                );
            }
            Opcode::Mos6502(Mos6502Opcode::Tya) => {
                self.patch_read_maybe_effective_address_dependent(
                    instruction,
                    [Phi2::Move {
                        source: MoveSource::Register {
                            register: GeneralPurposeRegister::Y,
                        },
                        destination: MoveDestination::Register {
                            register: GeneralPurposeRegister::A,
                            update_nz: true,
                        },
                    }],
                );
            }
            Opcode::Mos6502(Mos6502Opcode::Xaa) => {
                self.patch_read_maybe_effective_address_dependent(
                    instruction,
                    [
                        Phi2::Move {
                            source: MoveSource::Data,
                            destination: MoveDestination::Operand,
                        },
                        Phi2::UnstableAndWithMagicConstant,
                    ],
                );
            }
            Opcode::Mos6502(Mos6502Opcode::Bvs)
            | Opcode::Mos6502(Mos6502Opcode::Bvc)
            | Opcode::Mos6502(Mos6502Opcode::Beq)
            | Opcode::Mos6502(Mos6502Opcode::Bne)
            | Opcode::Mos6502(Mos6502Opcode::Bcs)
            | Opcode::Mos6502(Mos6502Opcode::Bcc)
            | Opcode::Mos6502(Mos6502Opcode::Bmi)
            | Opcode::Mos6502(Mos6502Opcode::Bpl)
            | Opcode::Wdc65C02(Wdc65C02Opcode::Bra) => {
                let branch_taken = match instruction.opcode {
                    Opcode::Mos6502(Mos6502Opcode::Bvs) => self.state.flags.overflow,
                    Opcode::Mos6502(Mos6502Opcode::Bvc) => !self.state.flags.overflow,
                    Opcode::Mos6502(Mos6502Opcode::Beq) => self.state.flags.zero,
                    Opcode::Mos6502(Mos6502Opcode::Bne) => !self.state.flags.zero,
                    Opcode::Mos6502(Mos6502Opcode::Bcs) => self.state.flags.carry,
                    Opcode::Mos6502(Mos6502Opcode::Bcc) => !self.state.flags.carry,
                    Opcode::Mos6502(Mos6502Opcode::Bmi) => self.state.flags.negative,
                    Opcode::Mos6502(Mos6502Opcode::Bpl) => !self.state.flags.negative,
                    Opcode::Wdc65C02(Wdc65C02Opcode::Bra) => true,
                    _ => unreachable!(),
                };

                if branch_taken {
                    self.patch_read_maybe_effective_address_dependent(
                        instruction,
                        [Phi2::Move {
                            source: MoveSource::Data,
                            destination: MoveDestination::Operand,
                        }],
                    );

                    self.state.cycle_queue.extend([Cycle::new(
                        BusMode::Read,
                        None,
                        [Phi2::AddToPointerLikeRegister {
                            adjustment: IndexAdjustment::OnCarry,
                            source: AddToPointerLikeRegisterSource::Operand,
                            destination: PointerLikeRegister::InstructionPointer,
                            interpretation: ArithmeticOperandInterpretation::Signed,
                        }],
                    )]);
                }
            }
            Opcode::Wdc65C02(Wdc65C02Opcode::Phx) => {
                self.push_stack_item(MoveSource::Register {
                    register: GeneralPurposeRegister::X,
                });
            }
            Opcode::Wdc65C02(Wdc65C02Opcode::Phy) => {
                self.push_stack_item(MoveSource::Register {
                    register: GeneralPurposeRegister::Y,
                });
            }
            Opcode::Wdc65C02(Wdc65C02Opcode::Plx) => {
                todo!()
            }
            Opcode::Wdc65C02(Wdc65C02Opcode::Ply) => {
                todo!()
            }
            Opcode::Wdc65C02(Wdc65C02Opcode::Stz) => {
                self.insert_write_effective_address_dependent(
                    instruction,
                    [Phi2::Move {
                        source: MoveSource::Constant(0),
                        destination: MoveDestination::Data,
                    }],
                );
            }
            Opcode::Wdc65C02(Wdc65C02Opcode::Trb) => {
                todo!()
            }
            Opcode::Wdc65C02(Wdc65C02Opcode::Tsb) => {
                todo!()
            }
            Opcode::Wdc65C02(Wdc65C02Opcode::Stp) => {
                todo!()
            }
            Opcode::Wdc65C02(Wdc65C02Opcode::Wai) => {
                todo!()
            }
        }
    }

    #[inline]
    fn index_adjustment(opcode: Opcode) -> IndexAdjustment {
        match opcode {
            Opcode::Mos6502(Mos6502Opcode::Sha) => IndexAdjustment::UnstableStore {
                source: UnstableStoreSource::AAndX,
            },
            Opcode::Mos6502(Mos6502Opcode::Shs) => IndexAdjustment::UnstableStore {
                source: UnstableStoreSource::StackPointerFromAAndX,
            },
            Opcode::Mos6502(Mos6502Opcode::Shx) => IndexAdjustment::UnstableStore {
                source: UnstableStoreSource::X,
            },
            Opcode::Mos6502(Mos6502Opcode::Shy) => IndexAdjustment::UnstableStore {
                source: UnstableStoreSource::Y,
            },
            // Anything that writes cannot put a byte somewhere it might have to take it back from
            Opcode::Mos6502(
                Mos6502Opcode::Asl
                | Mos6502Opcode::Dcp
                | Mos6502Opcode::Dec
                | Mos6502Opcode::Inc
                | Mos6502Opcode::Isc
                | Mos6502Opcode::Lsr
                | Mos6502Opcode::Rla
                | Mos6502Opcode::Rol
                | Mos6502Opcode::Ror
                | Mos6502Opcode::Rra
                | Mos6502Opcode::Sax
                | Mos6502Opcode::Slo
                | Mos6502Opcode::Sre
                | Mos6502Opcode::Sta
                | Mos6502Opcode::Stx
                | Mos6502Opcode::Sty,
            )
            | Opcode::Wdc65C02(Wdc65C02Opcode::Stz | Wdc65C02Opcode::Trb | Wdc65C02Opcode::Tsb) => {
                IndexAdjustment::Always
            }
            _ => IndexAdjustment::OnCarry,
        }
    }

    #[inline]
    fn register_indexed_zero_page(&mut self, register: GeneralPurposeRegister) {
        assert!(
            matches!(
                register,
                GeneralPurposeRegister::X | GeneralPurposeRegister::Y,
            ),
            "The A register cannot be used for indexing"
        );

        self.state.cycle_queue.extend([
            Cycle::new(
                BusMode::Read,
                Some(Phi1Source::InstructionPointer),
                [
                    Phi2::IncrementInstructionPointer,
                    Phi2::Move {
                        source: MoveSource::Data,
                        destination: MoveDestination::EffectiveAddress,
                    },
                ],
            ),
            Cycle::new(
                BusMode::Read,
                None,
                [Phi2::AddToPointerLikeRegister {
                    source: AddToPointerLikeRegisterSource::Register(register),
                    destination: PointerLikeRegister::EffectiveAddress,
                    interpretation: ArithmeticOperandInterpretation::Unsigned,
                    // Zero indexing automatically fixes the high byte via wrapping
                    adjustment: IndexAdjustment::Discard,
                }],
            ),
        ]);
    }

    #[inline]
    fn register_indexed_absolute(
        &mut self,
        register: GeneralPurposeRegister,
        adjustment: IndexAdjustment,
    ) {
        assert!(
            matches!(
                register,
                GeneralPurposeRegister::X | GeneralPurposeRegister::Y,
            ),
            "The A register cannot be used for indexing"
        );

        self.state.cycle_queue.extend([
            Cycle::new(
                BusMode::Read,
                Some(Phi1Source::InstructionPointer),
                [
                    Phi2::IncrementInstructionPointer,
                    Phi2::Move {
                        source: MoveSource::Data,
                        destination: MoveDestination::EffectiveAddress,
                    },
                ],
            ),
            Cycle::new(
                BusMode::Read,
                Some(Phi1Source::InstructionPointer),
                [
                    Phi2::IncrementInstructionPointer,
                    Phi2::Move {
                        source: MoveSource::Data,
                        destination: MoveDestination::EffectiveAddress,
                    },
                    Phi2::AddToPointerLikeRegister {
                        source: AddToPointerLikeRegisterSource::Register(register),
                        destination: PointerLikeRegister::EffectiveAddress,
                        adjustment,
                        interpretation: ArithmeticOperandInterpretation::Unsigned,
                    },
                ],
            ),
        ]);
    }

    #[inline]
    fn pull_stack_item(&mut self, item: MoveDestination) {
        self.state.cycle_queue.clear();

        self.state.cycle_queue.extend([
            Cycle::new(BusMode::Read, None, []),
            Cycle::new(
                BusMode::Read,
                None,
                [Phi2::IncrementStack { subtract: false }],
            ),
            Cycle::new(
                BusMode::Read,
                Some(Phi1Source::Stack),
                [Phi2::Move {
                    source: MoveSource::Data,
                    destination: item,
                }],
            ),
        ]);
    }

    #[inline]
    fn push_stack_item(&mut self, item: MoveSource) {
        self.state
            .cycle_queue
            .push_back(Cycle::new(
                BusMode::Write,
                Some(Phi1Source::Stack),
                [
                    Phi2::Move {
                        source: item,
                        destination: MoveDestination::Data,
                    },
                    Phi2::IncrementStack { subtract: true },
                ],
            ))
            .unwrap();
    }

    #[inline]
    fn patch_read_maybe_effective_address_dependent(
        &mut self,
        instruction: &Mos6502InstructionSet,
        steps: impl IntoIterator<Item = Phi2>,
    ) {
        match instruction.addressing_mode {
            // These instructions don't actually use the effective address system
            //
            // They either don't operate on memory or they operate on memory so implicit address resolution isn't done
            None
            | Some(AddressingMode::Mos6502(
                Mos6502AddressingMode::Accumulator
                | Mos6502AddressingMode::Immediate
                | Mos6502AddressingMode::Relative,
            )) => {
                // These instructions have a final semi-dummy cycle that can be leeched off
                self.state
                    .cycle_queue
                    .iter_mut()
                    .last()
                    .unwrap()
                    .phi2
                    .extend(steps);
            }
            _ => {
                self.state
                    .cycle_queue
                    .push_back(Cycle::new(
                        BusMode::Read,
                        Some(Phi1Source::EffectiveAddress),
                        steps,
                    ))
                    .unwrap();
            }
        }
    }

    #[inline]
    fn insert_rmw_effective_address_dependent(&mut self, steps: impl IntoIterator<Item = Phi2>) {
        self.state.cycle_queue.extend([
            Cycle::new(
                BusMode::Read,
                Some(Phi1Source::EffectiveAddress),
                [Phi2::Move {
                    source: MoveSource::Data,
                    destination: MoveDestination::Operand,
                }],
            ),
            Cycle::new(
                BusMode::Write,
                None,
                [Phi2::Move {
                    source: MoveSource::Operand,
                    destination: MoveDestination::Data,
                }],
            ),
            Cycle::new(
                BusMode::Write,
                None,
                steps.into_iter().chain(core::iter::once(Phi2::Move {
                    source: MoveSource::Operand,
                    destination: MoveDestination::Data,
                })),
            ),
        ]);
    }

    #[inline]
    fn insert_write_effective_address_dependent(
        &mut self,
        instruction: &Mos6502InstructionSet,
        steps: impl IntoIterator<Item = Phi2>,
    ) {
        match instruction.addressing_mode {
            // It's impossible to have an instruction that writes but does not form an effective address
            //
            // Additionally, merging with the previous cycle is impossible because all addressing mode resolution cycles are read
            None
            | Some(AddressingMode::Mos6502(
                Mos6502AddressingMode::Accumulator
                | Mos6502AddressingMode::Immediate
                | Mos6502AddressingMode::Relative,
            )) => {
                unreachable!()
            }
            _ => {
                self.state
                    .cycle_queue
                    .push_back(Cycle::new(
                        BusMode::Write,
                        Some(Phi1Source::EffectiveAddress),
                        steps,
                    ))
                    .unwrap();
            }
        }
    }
}
