use crate::{
    instruction::{AddressingMode, Mos6502AddressingMode, Mos6502Opcode, Opcode},
    variant::Variant,
};

#[inline]
pub fn decode_undocumented_space_instruction<V: Variant>(
    instruction_identifier: u8,
    argument: u8,
) -> (Opcode, Option<AddressingMode>) {
    // No UB instructions on the WDC 65C02
    if V::WDC_VARIANT {
        (Opcode::Mos6502(Mos6502Opcode::Nop), None)
    } else {
        let addressing_mode = AddressingMode::from_group1_addressing(argument);

        match instruction_identifier {
            0b000 => {
                if matches!(
                    addressing_mode,
                    AddressingMode::Mos6502(Mos6502AddressingMode::Immediate)
                ) {
                    (Opcode::Mos6502(Mos6502Opcode::Anc), Some(addressing_mode))
                } else {
                    (Opcode::Mos6502(Mos6502Opcode::Slo), Some(addressing_mode))
                }
            }
            0b001 => {
                if matches!(
                    addressing_mode,
                    AddressingMode::Mos6502(Mos6502AddressingMode::Immediate)
                ) {
                    (Opcode::Mos6502(Mos6502Opcode::Anc), Some(addressing_mode))
                } else {
                    (Opcode::Mos6502(Mos6502Opcode::Rla), Some(addressing_mode))
                }
            }
            0b010 => {
                if matches!(
                    addressing_mode,
                    AddressingMode::Mos6502(Mos6502AddressingMode::Immediate)
                ) {
                    (Opcode::Mos6502(Mos6502Opcode::Asr), Some(addressing_mode))
                } else {
                    (Opcode::Mos6502(Mos6502Opcode::Sre), Some(addressing_mode))
                }
            }
            0b011 => {
                if matches!(
                    addressing_mode,
                    AddressingMode::Mos6502(Mos6502AddressingMode::Immediate)
                ) {
                    (Opcode::Mos6502(Mos6502Opcode::Arr), Some(addressing_mode))
                } else {
                    (Opcode::Mos6502(Mos6502Opcode::Rra), Some(addressing_mode))
                }
            }
            0b100 => match addressing_mode {
                AddressingMode::Mos6502(Mos6502AddressingMode::XIndexedZeroPageIndirect) => {
                    (Opcode::Mos6502(Mos6502Opcode::Sax), Some(addressing_mode))
                }
                AddressingMode::Mos6502(Mos6502AddressingMode::Immediate) => {
                    (Opcode::Mos6502(Mos6502Opcode::Xaa), Some(addressing_mode))
                }
                AddressingMode::Mos6502(
                    Mos6502AddressingMode::Absolute | Mos6502AddressingMode::ZeroPage,
                ) => (Opcode::Mos6502(Mos6502Opcode::Sax), Some(addressing_mode)),
                AddressingMode::Mos6502(Mos6502AddressingMode::XIndexedZeroPage) => (
                    Opcode::Mos6502(Mos6502Opcode::Sax),
                    Some(AddressingMode::Mos6502(
                        Mos6502AddressingMode::YIndexedZeroPage,
                    )),
                ),
                AddressingMode::Mos6502(Mos6502AddressingMode::YIndexedAbsolute) => {
                    (Opcode::Mos6502(Mos6502Opcode::Shs), Some(addressing_mode))
                }
                AddressingMode::Mos6502(Mos6502AddressingMode::XIndexedAbsolute) => (
                    Opcode::Mos6502(Mos6502Opcode::Sha),
                    Some(AddressingMode::Mos6502(
                        Mos6502AddressingMode::YIndexedAbsolute,
                    )),
                ),
                AddressingMode::Mos6502(Mos6502AddressingMode::ZeroPageIndirectYIndexed) => {
                    (Opcode::Mos6502(Mos6502Opcode::Sha), Some(addressing_mode))
                }
                _ => unreachable!(),
            },
            0b101 => match addressing_mode {
                AddressingMode::Mos6502(Mos6502AddressingMode::YIndexedAbsolute) => {
                    (Opcode::Mos6502(Mos6502Opcode::Las), Some(addressing_mode))
                }
                AddressingMode::Mos6502(Mos6502AddressingMode::XIndexedZeroPage) => (
                    Opcode::Mos6502(Mos6502Opcode::Lax),
                    Some(AddressingMode::Mos6502(
                        Mos6502AddressingMode::YIndexedZeroPage,
                    )),
                ),
                AddressingMode::Mos6502(Mos6502AddressingMode::XIndexedAbsolute) => (
                    Opcode::Mos6502(Mos6502Opcode::Lax),
                    Some(AddressingMode::Mos6502(
                        Mos6502AddressingMode::YIndexedAbsolute,
                    )),
                ),
                _ => (Opcode::Mos6502(Mos6502Opcode::Lax), Some(addressing_mode)),
            },
            0b110 => {
                if matches!(
                    addressing_mode,
                    AddressingMode::Mos6502(Mos6502AddressingMode::Immediate)
                ) {
                    (Opcode::Mos6502(Mos6502Opcode::Sbx), Some(addressing_mode))
                } else {
                    (Opcode::Mos6502(Mos6502Opcode::Dcp), Some(addressing_mode))
                }
            }
            0b111 => {
                if matches!(
                    addressing_mode,
                    AddressingMode::Mos6502(Mos6502AddressingMode::Immediate)
                ) {
                    (Opcode::Mos6502(Mos6502Opcode::Sbc), Some(addressing_mode))
                } else {
                    (Opcode::Mos6502(Mos6502Opcode::Isc), Some(addressing_mode))
                }
            }
            _ => {
                unreachable!()
            }
        }
    }
}
