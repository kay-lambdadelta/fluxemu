use crate::{
    FlagRegister, Mos6502,
    cycle::{
        AddToPointerLikeRegisterSource, ArithmeticOperandInterpretation, BusMode, Cycle, Flag,
        GeneralPurposeRegister, IncrementOperand, MoveDestination, MoveSource, Phi2,
        PointerLikeRegister, ShiftDirection,
    },
    decoder::{
        InstructionGroup, decode_group1_space_instruction, decode_group2_space_instruction,
        decode_group3_space_instruction, decode_undocumented_space_instruction,
    },
    instruction::Mos6502InstructionSet,
};

impl Mos6502 {
    #[inline]
    pub(super) fn handle_phi2(&mut self, current_cycle: &Cycle) {
        for &step in current_cycle.phi2.iter() {
            match step {
                Phi2::AddToPointerLikeRegister {
                    insert_adjustment_cycle_upon_carry,
                    interpretation,
                    source,
                    destination,
                } => {
                    self.add_to_pointer_like_register(
                        insert_adjustment_cycle_upon_carry,
                        interpretation,
                        source,
                        destination,
                    );
                }
                Phi2::AddCarryToPointerLikeRegister { register, carry } => {
                    self.add_carry_to_pointer_like_register(register, carry);
                }
                Phi2::Move {
                    source,
                    destination,
                } => {
                    let value = match source {
                        MoveSource::Register { register } => match register {
                            GeneralPurposeRegister::A => self.state.a,
                            GeneralPurposeRegister::X => self.state.x,
                            GeneralPurposeRegister::Y => self.state.y,
                        },
                        MoveSource::Operand => self.state.operand,
                        MoveSource::Stack => self.state.stack,
                        MoveSource::Data => self.state.bus.data,
                        MoveSource::Constant(value) => value,
                        MoveSource::Flags { break_ } => self.state.flags.to_byte(break_),
                        MoveSource::InstructionPointer { offset } => {
                            self.state.instruction_pointer.to_le_bytes()[offset as usize]
                        }
                    };

                    match destination {
                        MoveDestination::Register {
                            register,
                            update_nz,
                        } => {
                            if update_nz {
                                self.state.flags.negative = (value as i8).is_negative();
                                self.state.flags.zero = value == 0;
                            }

                            match register {
                                GeneralPurposeRegister::A => self.state.a = value,
                                GeneralPurposeRegister::X => self.state.x = value,
                                GeneralPurposeRegister::Y => self.state.y = value,
                            }
                        }
                        MoveDestination::Operand => self.state.operand = value,
                        MoveDestination::Stack => self.state.stack = value,
                        MoveDestination::EffectiveAddress => {
                            self.state.effective_address.push(value).unwrap();
                        }
                        MoveDestination::Opcode => {
                            self.decode();
                        }
                        MoveDestination::Data => {
                            self.state.bus.data = value;
                        }
                        MoveDestination::Flags => {
                            self.state.flags = FlagRegister::from_byte(value);
                        }
                    }
                }
                Phi2::SetFlag { flag, value } => match flag {
                    Flag::Carry => self.state.flags.carry = value,
                    Flag::Zero => self.state.flags.zero = value,
                    Flag::Overflow => self.state.flags.overflow = value,
                    Flag::Negative => self.state.flags.negative = value,
                    Flag::Decimal => self.state.flags.decimal = value,
                    Flag::InterruptDisable => self.state.flags.interrupt_disable = value,
                },
                Phi2::LoadInstructionPointerFromEffectiveAddress => {
                    match self.state.effective_address.len() {
                        1 => {
                            self.state.instruction_pointer =
                                u16::from(self.state.effective_address[0]);
                        }
                        2 => {
                            self.state.instruction_pointer = u16::from_le_bytes([
                                self.state.effective_address[0],
                                self.state.effective_address[1],
                            ]);
                        }
                        _ => unreachable!(),
                    }

                    self.state.consume_effective_address = true;
                }
                Phi2::Increment { operand, subtract } => {
                    let operand = match operand {
                        IncrementOperand::X => &mut self.state.x,
                        IncrementOperand::Y => &mut self.state.y,
                        IncrementOperand::Operand => &mut self.state.operand,
                    };

                    let delta: i8 = if subtract { -1 } else { 1 };

                    *operand = operand.wrapping_add_signed(delta);

                    self.state.flags.negative = (*operand as i8).is_negative();
                    self.state.flags.zero = *operand == 0;
                }
                Phi2::Compare { register } => {
                    let value = match register {
                        GeneralPurposeRegister::A => self.state.a,
                        GeneralPurposeRegister::X => self.state.x,
                        GeneralPurposeRegister::Y => self.state.y,
                    };

                    let (result, carry) = value.overflowing_sub(self.state.operand);

                    self.state.flags.carry = !carry;
                    self.state.flags.zero = result == 0;
                    self.state.flags.negative = (result as i8).is_negative();
                }
                Phi2::IncrementStack { subtract } => {
                    self.state.stack = if subtract {
                        self.state.stack.wrapping_sub(1)
                    } else {
                        self.state.stack.wrapping_add(1)
                    };
                }
                Phi2::IncrementInstructionPointer => {
                    self.state.instruction_pointer = self.state.instruction_pointer.wrapping_add(1);
                }
                Phi2::And { writeback } => {
                    let result = self.state.a & self.state.operand;

                    self.state.flags.zero = result == 0;

                    if writeback {
                        self.state.a = result;

                        self.state.flags.negative = (result as i8).is_negative();
                    } else {
                        self.state.flags.negative = (self.state.operand as i8).is_negative();
                        self.state.flags.overflow = (self.state.operand & 0b0100_0000) != 0;
                    };
                }
                Phi2::Or => {
                    let result = self.state.a | self.state.operand;

                    self.state.flags.zero = result == 0;
                    self.state.flags.negative = (result as i8).is_negative();

                    self.state.a = result;
                }
                Phi2::Xor => {
                    let result = self.state.a ^ self.state.operand;

                    self.state.flags.zero = result == 0;
                    self.state.flags.negative = (result as i8).is_negative();

                    self.state.a = result;
                }
                Phi2::Shift {
                    direction,
                    rotate,
                    a_is_operand,
                } => {
                    let operand = if a_is_operand {
                        &mut self.state.a
                    } else {
                        &mut self.state.operand
                    };

                    let shift_input = if rotate {
                        self.state.flags.carry
                    } else {
                        false
                    };

                    match direction {
                        ShiftDirection::Left => {
                            let shift_output = (*operand & 0b1000_0000) != 0;
                            self.state.flags.carry = shift_output;

                            *operand = (*operand << 1) | (shift_input as u8);
                        }
                        ShiftDirection::Right => {
                            let shift_output = (*operand & 0b0000_0001) != 0;
                            self.state.flags.carry = shift_output;

                            *operand = (*operand >> 1) | ((shift_input as u8) << 7);
                        }
                    }

                    self.state.flags.zero = *operand == 0;
                    self.state.flags.negative = (*operand as i8).is_negative();
                }
                Phi2::Add { invert_operand } => {
                    let operand = if invert_operand {
                        !self.state.operand
                    } else {
                        self.state.operand
                    };

                    let (first_operation_result, first_operation_carry) =
                        self.state.a.overflowing_add(operand);

                    let (second_operation_result, second_operation_carry) =
                        first_operation_result.overflowing_add(self.state.flags.carry.into());

                    self.state.flags.overflow = ((self.state.a & 0b1000_0000)
                        == (operand & 0b1000_0000))
                        && ((self.state.a & 0b1000_0000)
                            != (second_operation_result & 0b1000_0000));

                    self.state.flags.carry = first_operation_carry || second_operation_carry;

                    self.state.flags.negative = (second_operation_result as i8).is_negative();

                    self.state.flags.zero = second_operation_result == 0;

                    self.state.a = second_operation_result;
                }
            }
        }
    }

    #[inline]
    fn add_carry_to_pointer_like_register(&mut self, register: PointerLikeRegister, carry: i8) {
        let address = match register {
            PointerLikeRegister::AddressBus => self.state.bus.address,
            PointerLikeRegister::InstructionPointer => self.state.instruction_pointer,
            PointerLikeRegister::EffectiveAddress => {
                match self.state.effective_address.len() {
                    // It would be impossible for a "1" to be here
                    2 => u16::from_le_bytes([
                        self.state.effective_address[0],
                        self.state.effective_address[1],
                    ]),
                    _ => unreachable!(),
                }
            }
        };

        let [address_low, address_high] = address.to_le_bytes();
        let result = address_high.wrapping_add_signed(carry);

        match register {
            PointerLikeRegister::AddressBus => {
                self.state.bus.address = u16::from_le_bytes([address_low, result]);
            }
            PointerLikeRegister::EffectiveAddress => {
                self.state.effective_address[0] = address_low;
                self.state.effective_address[1] = result;
            }
            PointerLikeRegister::InstructionPointer => {
                self.state.instruction_pointer = u16::from_le_bytes([address_low, result]);
            }
        }
    }

    #[inline]
    fn add_to_pointer_like_register(
        &mut self,
        insert_carry_cycle: bool,
        interpretation: ArithmeticOperandInterpretation,
        source: AddToPointerLikeRegisterSource,
        destination: PointerLikeRegister,
    ) {
        let mut carry = 0;

        let value = match source {
            AddToPointerLikeRegisterSource::Register(register) => match register {
                GeneralPurposeRegister::A => self.state.a,
                GeneralPurposeRegister::X => self.state.x,
                GeneralPurposeRegister::Y => self.state.y,
            },
            AddToPointerLikeRegisterSource::Constant(value) => value,
            AddToPointerLikeRegisterSource::Operand => self.state.operand,
        };

        let address = match destination {
            PointerLikeRegister::AddressBus => self.state.bus.address,
            PointerLikeRegister::EffectiveAddress => match self.state.effective_address.len() {
                1 => u16::from(self.state.effective_address[0]),
                2 => u16::from_le_bytes([
                    self.state.effective_address[0],
                    self.state.effective_address[1],
                ]),
                _ => unreachable!(),
            },
            PointerLikeRegister::InstructionPointer => self.state.instruction_pointer,
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
                self.state.bus.address = u16::from_le_bytes([result_low, address_high]);
            }
            PointerLikeRegister::InstructionPointer => {
                self.state.instruction_pointer = u16::from_le_bytes([result_low, address_high]);
            }
            PointerLikeRegister::EffectiveAddress => match self.state.effective_address.len() {
                1 => {
                    self.state.effective_address[0] = result_low;
                }
                2 => {
                    self.state.effective_address[0] = result_low;
                    self.state.effective_address[1] = address_high;
                }
                _ => {
                    unreachable!()
                }
            },
        }

        if carry != 0 && insert_carry_cycle {
            self.state
                .cycle_queue
                .push_front(Cycle::new(
                    BusMode::Read,
                    None,
                    [Phi2::AddCarryToPointerLikeRegister {
                        register: destination,
                        carry,
                    }],
                ))
                .unwrap();
        }
    }

    #[inline]
    fn decode(&mut self) {
        let instruction_identifier =
            InstructionGroup::from_repr(self.state.bus.data & 0b11).unwrap();
        let secondary_instruction_identifier = (self.state.bus.data >> 5) & 0b111;
        let argument = (self.state.bus.data >> 2) & 0b111;

        let (opcode, addressing_mode) = match instruction_identifier {
            InstructionGroup::Group3 => decode_group3_space_instruction(
                secondary_instruction_identifier,
                argument,
                self.config.kind,
            ),
            InstructionGroup::Group1 => decode_group1_space_instruction(
                secondary_instruction_identifier,
                argument,
                self.config.kind,
            ),
            InstructionGroup::Group2 => decode_group2_space_instruction(
                secondary_instruction_identifier,
                argument,
                self.config.kind,
            ),
            InstructionGroup::Undocumented => decode_undocumented_space_instruction(
                secondary_instruction_identifier,
                argument,
                self.config.kind,
            ),
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

        self.push_steps_for_instruction(&instruction);
    }
}
