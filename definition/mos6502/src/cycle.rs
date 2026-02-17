use arrayvec::ArrayVec;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum GeneralPurposeRegister {
    A,
    X,
    Y,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AddToPointerLikeRegisterSource {
    Register(GeneralPurposeRegister),
    Constant(u8),
    Operand,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BusMode {
    Read,
    Write,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MoveSource {
    Register {
        register: GeneralPurposeRegister,
    },
    Operand,
    Stack,
    Data,
    Constant(u8),
    Flags {
        break_: bool,
    },
    InstructionPointer {
        /// LITTLE ENDIAN
        offset: u8,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MoveDestination {
    Register {
        register: GeneralPurposeRegister,
        update_nz: bool,
    },
    Operand,
    Stack,
    EffectiveAddress,
    Opcode,
    Data,
    Flags,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Flag {
    Carry,
    Zero,
    Overflow,
    Negative,
    Decimal,
    InterruptDisable,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SetAddressBusSource {
    InstructionPointer,
    EffectiveAddress,
    Constant(u16),
    Stack,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PointerLikeRegister {
    AddressBus,
    EffectiveAddress,
    InstructionPointer,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ArithmeticOperandInterpretation {
    Unsigned,
    Signed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum IncrementOperand {
    X,
    Y,
    Operand,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ShiftDirection {
    Left,
    Right,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Phi1 {
    SetAddressBus { source: SetAddressBusSource },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Phi2 {
    Move {
        source: MoveSource,
        destination: MoveDestination,
    },
    SetFlag {
        flag: Flag,
        value: bool,
    },
    Increment {
        operand: IncrementOperand,
        subtract: bool,
    },
    Compare {
        register: GeneralPurposeRegister,
    },
    IncrementStack {
        subtract: bool,
    },
    Add {
        invert_operand: bool,
    },
    And {
        writeback: bool,
    },
    Or,
    Xor,
    Shift {
        direction: ShiftDirection,
        rotate: bool,
        a_is_operand: bool,
    },
    IncrementInstructionPointer,
    AddToPointerLikeRegister {
        insert_adjustment_cycle_upon_carry: bool,
        interpretation: ArithmeticOperandInterpretation,
        source: AddToPointerLikeRegisterSource,
        destination: PointerLikeRegister,
    },
    AddCarryToPointerLikeRegister {
        register: PointerLikeRegister,
        carry: i8,
    },
    LoadInstructionPointerFromEffectiveAddress,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Cycle {
    pub bus_mode: BusMode,
    pub phi1: Option<Phi1>,
    pub phi2: ArrayVec<Phi2, 3>,
}

impl Cycle {
    #[inline]
    pub fn new(
        bus_mode: BusMode,
        phi1: Option<Phi1>,
        phi2: impl IntoIterator<Item = Phi2>,
    ) -> Self {
        Self {
            bus_mode,
            phi1,
            phi2: phi2.into_iter().collect(),
        }
    }

    #[inline]
    pub fn dummy() -> Self {
        Self::new(BusMode::Read, None, [])
    }
}
