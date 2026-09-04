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
    AccumulatorAndX,
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
pub enum Phi1Source {
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
pub enum IndexAdjustment {
    /// The carry is always discarded and ignored
    Discard,
    /// The carry is serviced, consuming an extra cycle
    OnCarry,
    /// A cycle is always spent, regardless if carrying occurred or not
    Always,
    UnstableStore {
        source: UnstableStoreSource,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum UnstableStoreSource {
    /// Relevant to Sha
    AAndX,
    /// Relevant to Shx
    X,
    /// Relevant to Shy
    Y,
    /// Relevant to Shs
    StackPointerFromAAndX,
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
        adjustment: IndexAdjustment,
        interpretation: ArithmeticOperandInterpretation,
        source: AddToPointerLikeRegisterSource,
        destination: PointerLikeRegister,
    },
    AddCarryToPointerLikeRegister {
        register: PointerLikeRegister,
        carry: i8,
    },
    LoadInstructionPointerFromEffectiveAddress,
    // Below ones are relevant to only UB instructions
    CopyFlag {
        source: Flag,
        destination: Flag,
    },
    RotateRightThroughAdder,
    SubtractOperandFromAAndX,
    AndOperandWithStackPointer,
    UnstableAndWithMagicConstant,
    ComputeUnstableStoreOperand {
        source: UnstableStoreSource,
        register: PointerLikeRegister,
    },
    ReplacePointerLikeRegisterHighByteWithOperand {
        register: PointerLikeRegister,
    },
    Jam,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Cycle {
    pub bus_mode: BusMode,
    pub phi1: Option<Phi1Source>,
    pub phi2: heapless::Vec<Phi2, 4>,
}

impl Cycle {
    #[inline]
    pub fn new(
        bus_mode: BusMode,
        phi1: Option<Phi1Source>,
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
