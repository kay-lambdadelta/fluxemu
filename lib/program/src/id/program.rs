use std::{fmt::Display, path::PathBuf};

use percent_encoding::{AsciiSet, CONTROLS, utf8_percent_encode};
use serde::{Deserialize, Serialize};

use crate::SystemId;

const UNSAFE_PATH_CHARACTERS: &AsciiSet = &CONTROLS
    .add(b'%')
    .add(b'/')
    .add(b'\\')
    .add(b':')
    .add(b'*')
    .add(b'?')
    .add(b'"')
    .add(b'<')
    .add(b'>')
    .add(b'|');

/// A identifier for a program
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, PartialOrd, Ord)]
pub struct ProgramId {
    /// The system this program was produced for
    pub system: SystemId,
    /// A identifiable name for the program
    pub main_name: String,
}

impl ProgramId {
    pub fn path(&self) -> PathBuf {
        PathBuf::from(self.system.to_string())
            .join(utf8_percent_encode(&self.main_name, UNSAFE_PATH_CHARACTERS).to_string())
    }
}

impl Display for ProgramId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} <{}>", self.system, self.main_name)
    }
}

#[cfg(feature = "redb")]
impl redb::Value for ProgramId {
    type AsBytes<'a> = Vec<u8>;
    type SelfType<'a> = Self;

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
        redb::TypeName::new("program_id")
    }
}

#[cfg(feature = "redb")]
impl redb::Key for ProgramId {
    fn compare(data1: &[u8], data2: &[u8]) -> std::cmp::Ordering {
        data1.cmp(data2)
    }
}
