use std::fmt::Display;

use redb::{Key, TypeName, Value};
use serde::{Deserialize, Serialize};

use crate::MachineId;

/// A identifier for a program
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, PartialOrd, Ord)]
pub struct ProgramId {
    /// The machine this program was produced for
    pub machine: MachineId,
    /// A identifiable name for the program
    pub name: String,
}

impl Display for ProgramId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} {}", self.machine, self.name)
    }
}

impl Value for ProgramId {
    type SelfType<'a> = Self;

    type AsBytes<'a> = Vec<u8>;

    fn fixed_width() -> Option<usize> {
        None
    }

    fn from_bytes<'a>(data: &'a [u8]) -> Self::SelfType<'a>
    where
        Self: 'a,
    {
        rmp_serde::from_slice(data).unwrap()
    }

    fn as_bytes<'a, 'b: 'a>(value: &'a Self::SelfType<'b>) -> Self::AsBytes<'a>
    where
        Self: 'b,
    {
        rmp_serde::to_vec_named(value).unwrap()
    }

    fn type_name() -> redb::TypeName {
        TypeName::new("program_id")
    }
}

impl Key for ProgramId {
    fn compare(data1: &[u8], data2: &[u8]) -> std::cmp::Ordering {
        data1.cmp(data2)
    }
}
