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

/// An identifier for a program
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, PartialOrd, Ord, Hash)]
pub struct ProgramId {
    /// The system this program was produced for
    pub system: SystemId,
    /// An identifiable name for the program
    ///
    /// This will most often be the full dat name, and is the key many subsystems use
    ///
    /// This should be completely unique given the above [`SystemId`]
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, PartialOrd, Ord)]
pub struct GroupId(ProgramId);

impl GroupId {
    pub fn from_members<'a>(members: impl IntoIterator<Item = &'a ProgramId>) -> Option<Self> {
        members.into_iter().min().cloned().map(Self)
    }

    pub fn get(&self) -> &ProgramId {
        &self.0
    }
}
