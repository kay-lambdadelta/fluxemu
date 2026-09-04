use crate::{
    FlagRegister,
    component::Mos6502,
    cycle::{
        AddToPointerLikeRegisterSource, ArithmeticOperandInterpretation, BusMode, Cycle, Flag,
        GeneralPurposeRegister, IncrementOperand, IndexAdjustment, MoveDestination, MoveSource,
        Phi1Source, Phi2, PointerLikeRegister, ShiftDirection, UnstableStoreSource,
    },
    decoder::{
        InstructionGroup, decode_group1_space_instruction, decode_group2_space_instruction,
        decode_group3_space_instruction, decode_undocumented_space_instruction,
    },
    instruction::Mos6502InstructionSet,
    variant::Variant,
};

impl<V: Variant> Mos6502<V> {
    #[inline]
    pub(super) fn handle_phi2(&mut self, current_cycle: &Cycle) {
        for &step in current_cycle.phi2.iter() {
            match step {
                Phi2::AddToPointerLikeRegister {
                    adjustment,
                    interpretation,
                    source,
                    destination,
                } => {
                    self.add_to_pointer_like_register(
                        adjustment,
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
                        MoveSource::AccumulatorAndX => self.state.a & self.state.x,
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

                    let delta = if subtract { -1 } else { 1 };

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
                Phi2::CopyFlag {
                    source,
                    destination,
                } => {
                    let value = match source {
                        Flag::Carry => self.state.flags.carry,
                        Flag::Zero => self.state.flags.zero,
                        Flag::Overflow => self.state.flags.overflow,
                        Flag::Negative => self.state.flags.negative,
                        Flag::Decimal => self.state.flags.decimal,
                        Flag::InterruptDisable => self.state.flags.interrupt_disable,
                    };

                    match destination {
                        Flag::Carry => self.state.flags.carry = value,
                        Flag::Zero => self.state.flags.zero = value,
                        Flag::Overflow => self.state.flags.overflow = value,
                        Flag::Negative => self.state.flags.negative = value,
                        Flag::Decimal => self.state.flags.decimal = value,
                        Flag::InterruptDisable => self.state.flags.interrupt_disable = value,
                    }
                }
                Phi2::RotateRightThroughAdder => {
                    let result = (self.state.a >> 1) | ((self.state.flags.carry as u8) << 7);

                    let bit_5 = (result & 0b0010_0000) != 0;
                    let bit_6 = (result & 0b0100_0000) != 0;

                    self.state.flags.carry = bit_6;
                    self.state.flags.overflow = bit_6 != bit_5;
                    self.state.flags.negative = (result as i8).is_negative();
                    self.state.flags.zero = result == 0;

                    self.state.a = result;
                }
                Phi2::SubtractOperandFromAAndX => {
                    let (result, borrow) =
                        (self.state.a & self.state.x).overflowing_sub(self.state.operand);

                    self.state.flags.carry = !borrow;
                    self.state.flags.zero = result == 0;
                    self.state.flags.negative = (result as i8).is_negative();

                    self.state.x = result;
                }
                Phi2::AndOperandWithStackPointer => {
                    let result = self.state.stack & self.state.operand;

                    self.state.flags.zero = result == 0;
                    self.state.flags.negative = (result as i8).is_negative();

                    self.state.a = result;
                    self.state.x = result;
                    self.state.stack = result;
                }
                Phi2::UnstableAndWithMagicConstant => {
                    let result = (self.state.a | V::XAA_MAGIC_CONSTANT.unwrap())
                        & self.state.x
                        & self.state.operand;

                    self.state.flags.zero = result == 0;
                    self.state.flags.negative = (result as i8).is_negative();

                    self.state.a = result;
                }
                Phi2::ComputeUnstableStoreOperand { source, register } => {
                    let value = match source {
                        UnstableStoreSource::AAndX => self.state.a & self.state.x,
                        UnstableStoreSource::X => self.state.x,
                        UnstableStoreSource::Y => self.state.y,
                        UnstableStoreSource::StackPointerFromAAndX => {
                            self.state.stack = self.state.a & self.state.x;

                            self.state.stack
                        }
                    };

                    let [_, address_high] = self.pointer_like_register(register).to_le_bytes();

                    self.state.operand = value & address_high.wrapping_add(1);
                }
                Phi2::ReplacePointerLikeRegisterHighByteWithOperand { register } => {
                    let [address_low, _] = self.pointer_like_register(register).to_le_bytes();

                    self.write_pointer_like_register(
                        register,
                        u16::from_le_bytes([address_low, self.state.operand]),
                    );
                }
                Phi2::Jam => {
                    self.state
                        .cycle_queue
                        .push_front(Cycle::new(
                            BusMode::Read,
                            Some(Phi1Source::Constant(u16::MAX)),
                            [Phi2::Jam],
                        ))
                        .unwrap();
                }
            }
        }
    }

    #[inline]
    fn pointer_like_register(&self, register: PointerLikeRegister) -> u16 {
        match register {
            PointerLikeRegister::AddressBus => self.state.bus.address,
            PointerLikeRegister::InstructionPointer => self.state.instruction_pointer,
            PointerLikeRegister::EffectiveAddress => match self.state.effective_address.len() {
                1 => u16::from(self.state.effective_address[0]),
                2 => u16::from_le_bytes([
                    self.state.effective_address[0],
                    self.state.effective_address[1],
                ]),
                _ => unreachable!(),
            },
        }
    }

    #[inline]
    fn write_pointer_like_register(&mut self, register: PointerLikeRegister, value: u16) {
        let [value_low, value_high] = value.to_le_bytes();

        match register {
            PointerLikeRegister::AddressBus => self.state.bus.address = value,
            PointerLikeRegister::InstructionPointer => self.state.instruction_pointer = value,
            PointerLikeRegister::EffectiveAddress => match self.state.effective_address.len() {
                1 => {
                    self.state.effective_address[0] = value_low;
                }
                2 => {
                    self.state.effective_address[0] = value_low;
                    self.state.effective_address[1] = value_high;
                }
                _ => unreachable!(),
            },
        }
    }

    #[inline]
    fn add_carry_to_pointer_like_register(&mut self, register: PointerLikeRegister, carry: i8) {
        let [address_low, address_high] = self.pointer_like_register(register).to_le_bytes();
        let result = address_high.wrapping_add_signed(carry);

        self.write_pointer_like_register(register, u16::from_le_bytes([address_low, result]));
    }

    #[inline]
    fn add_to_pointer_like_register(
        &mut self,
        adjustment: IndexAdjustment,
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

        let address = self.pointer_like_register(destination);

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

        // This writes the OLD high so that the extra cycle can fix it if a carry arises and it
        // must be handled

        self.write_pointer_like_register(
            destination,
            u16::from_le_bytes([result_low, address_high]),
        );

        let adjustment_steps: heapless::Vec<_, 4> = match adjustment {
            IndexAdjustment::Discard => heapless::Vec::new(),
            IndexAdjustment::OnCarry if carry == 0 => heapless::Vec::new(),
            IndexAdjustment::OnCarry | IndexAdjustment::Always => {
                heapless::Vec::from_iter([Phi2::AddCarryToPointerLikeRegister {
                    register: destination,
                    carry,
                }])
            }
            IndexAdjustment::UnstableStore { source } => {
                let mut steps = heapless::Vec::from_iter([Phi2::ComputeUnstableStoreOperand {
                    source,
                    register: destination,
                }]);

                if carry != 0 {
                    steps
                        .push(Phi2::ReplacePointerLikeRegisterHighByteWithOperand {
                            register: destination,
                        })
                        .unwrap();
                }

                steps
            }
        };

        if !adjustment_steps.is_empty() {
            self.state
                .cycle_queue
                .push_front(Cycle::new(BusMode::Read, None, adjustment_steps))
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
            InstructionGroup::Group3 => {
                decode_group3_space_instruction(secondary_instruction_identifier, argument)
            }
            InstructionGroup::Group1 => {
                decode_group1_space_instruction::<V>(secondary_instruction_identifier, argument)
            }
            InstructionGroup::Group2 => {
                decode_group2_space_instruction::<V>(secondary_instruction_identifier, argument)
            }
            InstructionGroup::Undocumented => decode_undocumented_space_instruction::<V>(
                secondary_instruction_identifier,
                argument,
            ),
        };

        let instruction = Mos6502InstructionSet {
            opcode,
            addressing_mode,
        };

        assert!(
            instruction
                .addressing_mode
                .is_none_or(|addressing_mode| { V::is_addressing_mode_valid(&addressing_mode) }),
            "Invalid addressing mode for instruction for mode: {:?}",
            instruction,
        );

        self.push_steps_for_instruction(&instruction);
    }
}
