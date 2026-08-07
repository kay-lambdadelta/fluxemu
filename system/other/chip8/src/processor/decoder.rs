use nalgebra::Point2;

use super::instruction::{
    Chip8InstructionSet, InstructionSetChip8, InstructionSetSuperChip8, Register, ScrollDirection,
};

pub(super) fn decode_instruction(instruction: [u8; 2]) -> Option<Chip8InstructionSet> {
    let instruction = u16::from_be_bytes(instruction);

    let get_nibble = |n: u8| -> u8 { ((instruction >> (12 - n * 4)) & 0xf) as u8 };

    let nnn = instruction & 0x0fff;
    let kk = (instruction & 0x00ff) as u8;

    let nibbles = [get_nibble(0), get_nibble(1), get_nibble(2), get_nibble(3)];

    Some(match nibbles[0] {
        0x0 => match [nibbles[1], nibbles[2], nibbles[3]] {
            [0x0, 0xe, 0x0] => Chip8InstructionSet::Chip8(InstructionSetChip8::Clr),
            [0x0, 0xe, 0xe] => Chip8InstructionSet::Chip8(InstructionSetChip8::Rtrn),
            [0x0, 0xf, 0xe] => Chip8InstructionSet::SuperChip8(InstructionSetSuperChip8::Lores),
            [0x0, 0xf, 0xf] => Chip8InstructionSet::SuperChip8(InstructionSetSuperChip8::Hires),
            [0x0, 0xc, _] => Chip8InstructionSet::SuperChip8(InstructionSetSuperChip8::Scroll {
                direction: ScrollDirection::Down { amount: nibbles[3] },
            }),
            [0x0, 0xf, 0xb] => Chip8InstructionSet::SuperChip8(InstructionSetSuperChip8::Scroll {
                direction: ScrollDirection::Right,
            }),
            [0x0, 0xf, 0xc] => Chip8InstructionSet::SuperChip8(InstructionSetSuperChip8::Scroll {
                direction: ScrollDirection::Right,
            }),
            _ => return None,
        },
        0x1 => Chip8InstructionSet::Chip8(InstructionSetChip8::Jump { address: nnn }),
        0x2 => Chip8InstructionSet::Chip8(InstructionSetChip8::Call { address: nnn }),
        0x3 => Chip8InstructionSet::Chip8(InstructionSetChip8::Ske {
            register: Register::from_repr(nibbles[1]).unwrap(),
            immediate: kk,
        }),
        0x4 => Chip8InstructionSet::Chip8(InstructionSetChip8::Skne {
            register: Register::from_repr(nibbles[1]).unwrap(),
            immediate: kk,
        }),
        0x5 => Chip8InstructionSet::Chip8(InstructionSetChip8::Skre {
            param_1: Register::from_repr(nibbles[1]).unwrap(),
            param_2: Register::from_repr(nibbles[2]).unwrap(),
        }),
        0x6 => Chip8InstructionSet::Chip8(InstructionSetChip8::Load {
            register: Register::from_repr(nibbles[1]).unwrap(),
            immediate: kk,
        }),
        0x7 => Chip8InstructionSet::Chip8(InstructionSetChip8::Add {
            register: Register::from_repr(nibbles[1]).unwrap(),
            immediate: kk,
        }),
        0x8 => match nibbles[3] {
            0x0 => Chip8InstructionSet::Chip8(InstructionSetChip8::Move {
                param_1: Register::from_repr(nibbles[1]).unwrap(),
                param_2: Register::from_repr(nibbles[2]).unwrap(),
            }),
            0x1 => Chip8InstructionSet::Chip8(InstructionSetChip8::Or {
                destination: Register::from_repr(nibbles[1]).unwrap(),
                source: Register::from_repr(nibbles[2]).unwrap(),
            }),
            0x2 => Chip8InstructionSet::Chip8(InstructionSetChip8::And {
                destination: Register::from_repr(nibbles[1]).unwrap(),
                source: Register::from_repr(nibbles[2]).unwrap(),
            }),
            0x3 => Chip8InstructionSet::Chip8(InstructionSetChip8::Xor {
                destination: Register::from_repr(nibbles[1]).unwrap(),
                source: Register::from_repr(nibbles[2]).unwrap(),
            }),
            0x4 => Chip8InstructionSet::Chip8(InstructionSetChip8::Addr {
                destination: Register::from_repr(nibbles[1]).unwrap(),
                source: Register::from_repr(nibbles[2]).unwrap(),
            }),
            0x5 => Chip8InstructionSet::Chip8(InstructionSetChip8::Sub {
                destination: Register::from_repr(nibbles[1]).unwrap(),
                source: Register::from_repr(nibbles[2]).unwrap(),
            }),
            0x6 => Chip8InstructionSet::Chip8(InstructionSetChip8::Shr {
                register: Register::from_repr(nibbles[1]).unwrap(),
                value: Register::from_repr(nibbles[2]).unwrap(),
            }),
            0x7 => Chip8InstructionSet::Chip8(InstructionSetChip8::Subn {
                destination: Register::from_repr(nibbles[1]).unwrap(),
                source: Register::from_repr(nibbles[2]).unwrap(),
            }),
            0xe => Chip8InstructionSet::Chip8(InstructionSetChip8::Shl {
                register: Register::from_repr(nibbles[1]).unwrap(),
                value: Register::from_repr(nibbles[2]).unwrap(),
            }),
            _ => return None,
        },
        0x9 => match nibbles[3] {
            0x0 => Chip8InstructionSet::Chip8(InstructionSetChip8::Skrne {
                param_1: Register::from_repr(nibbles[1]).unwrap(),
                param_2: Register::from_repr(nibbles[2]).unwrap(),
            }),
            _ => return None,
        },
        0xa => Chip8InstructionSet::Chip8(InstructionSetChip8::Loadi { value: nnn }),
        0xb => Chip8InstructionSet::Chip8(InstructionSetChip8::Jumpi { address: nnn }),
        0xc => Chip8InstructionSet::Chip8(InstructionSetChip8::Rand {
            register: Register::from_repr(nibbles[1]).unwrap(),
            immediate: kk,
        }),
        0xd => Chip8InstructionSet::Chip8(InstructionSetChip8::Draw {
            coordinates: Point2::new(
                Register::from_repr(nibbles[1]).unwrap(),
                Register::from_repr(nibbles[2]).unwrap(),
            ),
            height: nibbles[3],
        }),
        0xe => match kk {
            0x9e => Chip8InstructionSet::Chip8(InstructionSetChip8::Skpr {
                key: Register::from_repr(nibbles[1]).unwrap(),
            }),
            0xa1 => Chip8InstructionSet::Chip8(InstructionSetChip8::Skup {
                key: Register::from_repr(nibbles[1]).unwrap(),
            }),
            _ => return None,
        },
        0xf => match kk {
            0x07 => Chip8InstructionSet::Chip8(InstructionSetChip8::Moved {
                register: Register::from_repr(nibbles[1]).unwrap(),
            }),
            0x0a => Chip8InstructionSet::Chip8(InstructionSetChip8::Keyd {
                key: Register::from_repr(nibbles[1]).unwrap(),
            }),
            0x15 => Chip8InstructionSet::Chip8(InstructionSetChip8::Loadd {
                register: Register::from_repr(nibbles[1]).unwrap(),
            }),
            0x18 => Chip8InstructionSet::Chip8(InstructionSetChip8::Loads {
                register: Register::from_repr(nibbles[1]).unwrap(),
            }),
            0x1e => Chip8InstructionSet::Chip8(InstructionSetChip8::Addi {
                register: Register::from_repr(nibbles[1]).unwrap(),
            }),
            0x29 => Chip8InstructionSet::Chip8(InstructionSetChip8::Font {
                register: Register::from_repr(nibbles[1]).unwrap(),
            }),
            0x33 => Chip8InstructionSet::Chip8(InstructionSetChip8::Bcd {
                register: Register::from_repr(nibbles[1]).unwrap(),
            }),
            0x55 => Chip8InstructionSet::Chip8(InstructionSetChip8::Save { count: nibbles[1] }),
            0x65 => Chip8InstructionSet::Chip8(InstructionSetChip8::Restore { count: nibbles[1] }),
            _ => return None,
        },
        _ => unreachable!(),
    })
}
