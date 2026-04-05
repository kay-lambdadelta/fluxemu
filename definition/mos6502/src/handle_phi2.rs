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
        for step in current_cycle.phi2.clone() {
            match step {
                Phi2::AddToPointerLikeRegister {
                    insert_adjustment_cycle_upon_carry: insert_carry_cycle,
                    interpretation,
                    source,
                    destination,
                } => {
                    self.add_to_pointer_like_register(
                        insert_carry_cycle,
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
                            self.effective_address.push(value).unwrap();
                        }
                        MoveDestination::Opcode => {
                            self.decode();
                        }
                        MoveDestination::Data => {
                            self.bus.data = value;
                        }
                        MoveDestination::Flags => {
                            self.flags = FlagRegister::from_byte(value);
                        }
                    }
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
                            ]);
                        }
                        _ => unreachable!(),
                    }

                    self.consume_effective_address = true;
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
                        first_operation_result.overflowing_add(self.flags.carry.into());

                    self.flags.overflow = ((self.a & 0b1000_0000) == (operand & 0b1000_0000))
                        && ((self.a & 0b1000_0000) != (second_operation_result & 0b1000_0000));

                    self.flags.carry = first_operation_carry || second_operation_carry;

                    self.flags.negative = (second_operation_result as i8).is_negative();

                    self.flags.zero = second_operation_result == 0;

                    self.a = second_operation_result;
                }
            }
        }
    }

    #[inline]
    fn add_carry_to_pointer_like_register(&mut self, register: PointerLikeRegister, carry: i8) {
        let address = match register {
            PointerLikeRegister::AddressBus => self.bus.address,
            PointerLikeRegister::InstructionPointer => self.instruction_pointer,
            PointerLikeRegister::EffectiveAddress => {
                match self.effective_address.len() {
                    // It would be impossible for a "1" to be here
                    2 => u16::from_le_bytes([self.effective_address[0], self.effective_address[1]]),
                    _ => unreachable!(),
                }
            }
        };

        let [address_low, address_high] = address.to_le_bytes();
        let result = address_high.wrapping_add_signed(carry);

        match register {
            PointerLikeRegister::AddressBus => {
                self.bus.address = u16::from_le_bytes([address_low, result]);
            }
            PointerLikeRegister::EffectiveAddress => {
                self.effective_address[0] = address_low;
                self.effective_address[1] = result;
            }
            PointerLikeRegister::InstructionPointer => {
                self.instruction_pointer = u16::from_le_bytes([address_low, result]);
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
                GeneralPurposeRegister::A => self.a,
                GeneralPurposeRegister::X => self.x,
                GeneralPurposeRegister::Y => self.y,
            },
            AddToPointerLikeRegisterSource::Constant(value) => value,
            AddToPointerLikeRegisterSource::Operand => self.operand,
        };

        let address = match destination {
            PointerLikeRegister::AddressBus => self.bus.address,
            PointerLikeRegister::EffectiveAddress => match self.effective_address.len() {
                1 => u16::from(self.effective_address[0]),
                2 => u16::from_le_bytes([self.effective_address[0], self.effective_address[1]]),
                _ => unreachable!(),
            },
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
                self.bus.address = u16::from_le_bytes([result_low, address_high]);
            }
            PointerLikeRegister::InstructionPointer => {
                self.instruction_pointer = u16::from_le_bytes([result_low, address_high]);
            }
            PointerLikeRegister::EffectiveAddress => match self.effective_address.len() {
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
            },
        }

        if carry != 0 && insert_carry_cycle {
            self.cycle_queue
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
        let instruction_identifier = InstructionGroup::from_repr(self.bus.data & 0b11).unwrap();
        let secondary_instruction_identifier = (self.bus.data >> 5) & 0b111;
        let argument = (self.bus.data >> 2) & 0b111;

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

        tracing::trace!(
            "Decoded instruction {:?} at address {:x}",
            instruction,
            self.instruction_pointer.wrapping_sub(1)
        );

        self.push_steps_for_instruction(&instruction);
    }
}
