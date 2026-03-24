use std::fmt::Display;

use fluxemu_runtime::processor::InstructionSet;
use serde::{Deserialize, Serialize};

use crate::{
    Mos6502, Mos6502Kind,
    cycle::{
        AddToPointerLikeRegisterSource, ArithmeticOperandInterpretation, BusMode, Cycle, Flag,
        GeneralPurposeRegister, IncrementOperand, MoveDestination, MoveSource, Phi1, Phi2,
        PointerLikeRegister, SetAddressBusSource, ShiftDirection,
    },
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

impl AddressingMode {
    pub fn is_valid_for_mode(&self, mode: Mos6502Kind) -> bool {
        match mode {
            Mos6502Kind::Mos6502 => matches!(self, AddressingMode::Mos6502(_)),
            Mos6502Kind::Mos6507 => matches!(self, AddressingMode::Mos6502(_)),
            Mos6502Kind::Ricoh2A0x => matches!(self, AddressingMode::Mos6502(_)),
            Mos6502Kind::Wdc65C02 => matches!(
                self,
                AddressingMode::Mos6502(_) | AddressingMode::Wdc65C02(_)
            ),
        }
    }
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
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
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

impl InstructionSet for Mos6502InstructionSet {
    type Opcode = Opcode;
    type AddressingMode = AddressingMode;
}

impl Mos6502 {
    pub(super) fn push_steps_for_instruction(&mut self, instruction: &Mos6502InstructionSet) {
        if let Some(addressing_mode) = instruction.addressing_mode {
            match addressing_mode {
                AddressingMode::Mos6502(Mos6502AddressingMode::Absolute) => {
                    self.instruction_queue.extend([
                        Cycle::new(
                            BusMode::Read,
                            Some(Phi1::SetAddressBus {
                                source: SetAddressBusSource::InstructionPointer,
                            }),
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
                            Some(Phi1::SetAddressBus {
                                source: SetAddressBusSource::InstructionPointer,
                            }),
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
                    self.instruction_queue.extend([Cycle::new(
                        BusMode::Read,
                        Some(Phi1::SetAddressBus {
                            source: SetAddressBusSource::InstructionPointer,
                        }),
                        [Phi2::IncrementInstructionPointer],
                    )]);
                }
                AddressingMode::Mos6502(Mos6502AddressingMode::XIndexedAbsolute) => {
                    self.register_indexed_absolute(GeneralPurposeRegister::X);
                }
                AddressingMode::Mos6502(Mos6502AddressingMode::YIndexedAbsolute) => {
                    self.register_indexed_absolute(GeneralPurposeRegister::Y);
                }
                AddressingMode::Mos6502(Mos6502AddressingMode::AbsoluteIndirect) => {
                    self.instruction_queue.extend([
                        Cycle::new(
                            BusMode::Read,
                            Some(Phi1::SetAddressBus {
                                source: SetAddressBusSource::InstructionPointer,
                            }),
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
                            Some(Phi1::SetAddressBus {
                                source: SetAddressBusSource::InstructionPointer,
                            }),
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
                            Some(Phi1::SetAddressBus {
                                source: SetAddressBusSource::EffectiveAddress,
                            }),
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
                                    insert_adjustment_cycle_upon_carry: !self
                                        .config
                                        .kind
                                        .has_absolute_indirect_page_wrap_errata(),
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
                    self.instruction_queue.extend([
                        Cycle::new(
                            BusMode::Read,
                            Some(Phi1::SetAddressBus {
                                source: SetAddressBusSource::InstructionPointer,
                            }),
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
                            Some(Phi1::SetAddressBus {
                                source: SetAddressBusSource::EffectiveAddress,
                            }),
                            [Phi2::AddToPointerLikeRegister {
                                source: AddToPointerLikeRegisterSource::Register(
                                    GeneralPurposeRegister::X,
                                ),
                                destination: PointerLikeRegister::AddressBus,
                                insert_adjustment_cycle_upon_carry: false,
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
                                    insert_adjustment_cycle_upon_carry: false,
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
                    self.instruction_queue.extend([
                        Cycle::new(
                            BusMode::Read,
                            Some(Phi1::SetAddressBus {
                                source: SetAddressBusSource::InstructionPointer,
                            }),
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
                            Some(Phi1::SetAddressBus {
                                source: SetAddressBusSource::EffectiveAddress,
                            }),
                            [
                                Phi2::Move {
                                    source: MoveSource::Data,
                                    destination: MoveDestination::EffectiveAddress,
                                },
                                Phi2::AddToPointerLikeRegister {
                                    source: AddToPointerLikeRegisterSource::Constant(1),
                                    destination: PointerLikeRegister::AddressBus,
                                    interpretation: ArithmeticOperandInterpretation::Unsigned,
                                    insert_adjustment_cycle_upon_carry: false,
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
                                    insert_adjustment_cycle_upon_carry: true,
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
                    self.instruction_queue.extend([Cycle::new(
                        BusMode::Read,
                        Some(Phi1::SetAddressBus {
                            source: SetAddressBusSource::InstructionPointer,
                        }),
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
                    self.instruction_queue.extend([Cycle::dummy()]);
                }
                AddressingMode::Wdc65C02(Wdc65C02AddressingMode::ZeroPageIndirect) => {
                    todo!()
                }
            }
        } else {
            self.instruction_queue.extend([Cycle::dummy()]);
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
            Opcode::Mos6502(Mos6502Opcode::Anc) => todo!(),
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
            Opcode::Mos6502(Mos6502Opcode::Arr) => todo!(),
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
            Opcode::Mos6502(Mos6502Opcode::Asr) => todo!(),
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
                todo!()
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
            Opcode::Mos6502(Mos6502Opcode::Dcp) => todo!(),
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
            Opcode::Mos6502(Mos6502Opcode::Isc) => todo!(),
            Opcode::Mos6502(Mos6502Opcode::Jam) => todo!(),
            Opcode::Mos6502(Mos6502Opcode::Jmp) => {
                // Note that this is correct for all actual existing addressing modes for JMP
                self.instruction_queue
                    .iter_mut()
                    .last()
                    .unwrap()
                    .phi2
                    .push(Phi2::LoadInstructionPointerFromEffectiveAddress);
            }
            Opcode::Mos6502(Mos6502Opcode::Jsr) => {
                self.instruction_queue.clear();

                self.instruction_queue.extend([
                    Cycle::new(
                        BusMode::Read,
                        Some(Phi1::SetAddressBus {
                            source: SetAddressBusSource::InstructionPointer,
                        }),
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
                        BusMode::Read,
                        Some(Phi1::SetAddressBus {
                            source: SetAddressBusSource::InstructionPointer,
                        }),
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
            Opcode::Mos6502(Mos6502Opcode::Las) => todo!(),
            Opcode::Mos6502(Mos6502Opcode::Lax) => todo!(),
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
                // Nothing happens
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
            Opcode::Mos6502(Mos6502Opcode::Rla) => todo!(),
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
            Opcode::Mos6502(Mos6502Opcode::Rra) => todo!(),
            Opcode::Mos6502(Mos6502Opcode::Rti) => {
                self.instruction_queue.clear();

                self.instruction_queue.extend([
                    Cycle::new(
                        BusMode::Read,
                        None,
                        [Phi2::IncrementStack { subtract: false }],
                    ),
                    Cycle::new(
                        BusMode::Read,
                        Some(Phi1::SetAddressBus {
                            source: SetAddressBusSource::Stack,
                        }),
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
                        Some(Phi1::SetAddressBus {
                            source: SetAddressBusSource::Stack,
                        }),
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
                        Some(Phi1::SetAddressBus {
                            source: SetAddressBusSource::Stack,
                        }),
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
                self.instruction_queue.clear();

                self.instruction_queue.extend([
                    Cycle::new(
                        BusMode::Read,
                        None,
                        [Phi2::IncrementStack { subtract: false }],
                    ),
                    Cycle::new(
                        BusMode::Read,
                        Some(Phi1::SetAddressBus {
                            source: SetAddressBusSource::Stack,
                        }),
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
                        Some(Phi1::SetAddressBus {
                            source: SetAddressBusSource::Stack,
                        }),
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
            Opcode::Mos6502(Mos6502Opcode::Sax) => todo!(),
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
            Opcode::Mos6502(Mos6502Opcode::Sbx) => todo!(),
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
            Opcode::Mos6502(Mos6502Opcode::Sha) => todo!(),
            Opcode::Mos6502(Mos6502Opcode::Shs) => todo!(),
            Opcode::Mos6502(Mos6502Opcode::Shx) => todo!(),
            Opcode::Mos6502(Mos6502Opcode::Shy) => todo!(),
            Opcode::Mos6502(Mos6502Opcode::Slo) => todo!(),
            Opcode::Mos6502(Mos6502Opcode::Sre) => todo!(),
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
            Opcode::Mos6502(Mos6502Opcode::Xaa) => todo!(),
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
                    Opcode::Mos6502(Mos6502Opcode::Bvs) => self.flags.overflow,
                    Opcode::Mos6502(Mos6502Opcode::Bvc) => !self.flags.overflow,
                    Opcode::Mos6502(Mos6502Opcode::Beq) => self.flags.zero,
                    Opcode::Mos6502(Mos6502Opcode::Bne) => !self.flags.zero,
                    Opcode::Mos6502(Mos6502Opcode::Bcs) => self.flags.carry,
                    Opcode::Mos6502(Mos6502Opcode::Bcc) => !self.flags.carry,
                    Opcode::Mos6502(Mos6502Opcode::Bmi) => self.flags.negative,
                    Opcode::Mos6502(Mos6502Opcode::Bpl) => !self.flags.negative,
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

                    self.instruction_queue.extend([Cycle::new(
                        BusMode::Read,
                        None,
                        [Phi2::AddToPointerLikeRegister {
                            insert_adjustment_cycle_upon_carry: true,
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
    fn register_indexed_zero_page(&mut self, register: GeneralPurposeRegister) {
        assert!(
            matches!(
                register,
                GeneralPurposeRegister::X | GeneralPurposeRegister::Y,
            ),
            "The A register cannot be used for indexing"
        );

        self.instruction_queue.extend([
            Cycle::new(
                BusMode::Read,
                Some(Phi1::SetAddressBus {
                    source: SetAddressBusSource::InstructionPointer,
                }),
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
                    insert_adjustment_cycle_upon_carry: false,
                }],
            ),
        ]);
    }

    #[inline]
    fn register_indexed_absolute(&mut self, register: GeneralPurposeRegister) {
        assert!(
            matches!(
                register,
                GeneralPurposeRegister::X | GeneralPurposeRegister::Y,
            ),
            "The A register cannot be used for indexing"
        );

        self.instruction_queue.extend([
            Cycle::new(
                BusMode::Read,
                Some(Phi1::SetAddressBus {
                    source: SetAddressBusSource::InstructionPointer,
                }),
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
                Some(Phi1::SetAddressBus {
                    source: SetAddressBusSource::InstructionPointer,
                }),
                [
                    Phi2::IncrementInstructionPointer,
                    Phi2::Move {
                        source: MoveSource::Data,
                        destination: MoveDestination::EffectiveAddress,
                    },
                    Phi2::AddToPointerLikeRegister {
                        source: AddToPointerLikeRegisterSource::Register(register),
                        destination: PointerLikeRegister::EffectiveAddress,
                        insert_adjustment_cycle_upon_carry: true,
                        interpretation: ArithmeticOperandInterpretation::Unsigned,
                    },
                ],
            ),
        ]);
    }

    #[inline]
    fn pull_stack_item(&mut self, item: MoveDestination) {
        self.instruction_queue.clear();

        self.instruction_queue.extend([
            Cycle::new(BusMode::Read, None, []),
            Cycle::new(
                BusMode::Read,
                None,
                [Phi2::IncrementStack { subtract: false }],
            ),
            Cycle::new(
                BusMode::Read,
                Some(Phi1::SetAddressBus {
                    source: SetAddressBusSource::Stack,
                }),
                [Phi2::Move {
                    source: MoveSource::Data,
                    destination: item,
                }],
            ),
        ]);
    }

    #[inline]
    fn push_stack_item(&mut self, item: MoveSource) {
        self.instruction_queue.push_back(Cycle::new(
            BusMode::Write,
            Some(Phi1::SetAddressBus {
                source: SetAddressBusSource::Stack,
            }),
            [
                Phi2::Move {
                    source: item,
                    destination: MoveDestination::Data,
                },
                Phi2::IncrementStack { subtract: true },
            ],
        ));
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
                self.instruction_queue
                    .iter_mut()
                    .last()
                    .unwrap()
                    .phi2
                    .extend(steps);
            }
            _ => {
                self.instruction_queue.push_back(Cycle::new(
                    BusMode::Read,
                    Some(Phi1::SetAddressBus {
                        source: SetAddressBusSource::EffectiveAddress,
                    }),
                    steps,
                ));
            }
        }
    }

    #[inline]
    fn insert_rmw_effective_address_dependent(&mut self, steps: impl IntoIterator<Item = Phi2>) {
        self.instruction_queue.extend([
            Cycle::new(
                BusMode::Read,
                Some(Phi1::SetAddressBus {
                    source: SetAddressBusSource::EffectiveAddress,
                }),
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
                steps.into_iter().chain(std::iter::once(Phi2::Move {
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
            // It's impossible to have a instruction that writes but does not form an effective address
            //
            // Additionally merging with the previous cycle is impossible because all addressing mode resolution cycles are read
            None
            | Some(AddressingMode::Mos6502(
                Mos6502AddressingMode::Accumulator
                | Mos6502AddressingMode::Immediate
                | Mos6502AddressingMode::Relative,
            )) => {
                unreachable!()
            }
            _ => {
                self.instruction_queue.push_back(Cycle::new(
                    BusMode::Write,
                    Some(Phi1::SetAddressBus {
                        source: SetAddressBusSource::EffectiveAddress,
                    }),
                    steps,
                ));
            }
        }
    }
}
