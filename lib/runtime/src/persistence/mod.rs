use thiserror::Error;

use crate::{ComponentPath, ResourcePath};

mod archive;
mod random;
mod save;
mod section;
mod snapshot;

pub(crate) use archive::ArchiveKind;
pub use random::PersistentRandomGenerator;
pub(crate) use section::ComponentCodec;
pub use section::{PersistentComponentState, Section};
pub(crate) use snapshot::{ComponentRecord, TaskTiming};

#[derive(Debug, Error)]
pub enum SnapshotError {
    #[error("{0}")]
    Encode(#[from] rmp_serde::encode::Error),
    #[error("{0}")]
    Decode(#[from] rmp_serde::decode::Error),
    #[error("Not a FluxEMU archive")]
    NotAnArchive,
    #[error("Unsupported archive format {0}")]
    UnsupportedFormat(u32),
    #[error("Archive is the wrong kind")]
    WrongKind,
    #[error("Archive is corrupt")]
    Corrupt,
    #[error("Archive entry is too large")]
    EntryTooLarge,
    #[error("Archive was made for a different program")]
    WrongProgram,
    #[error("Component {0} does not exist in this machine")]
    UnknownComponent(ComponentPath),
    #[error("Archive has no entry for component {0}")]
    MissingComponent(ComponentPath),
    #[error("Archive has no entry for memory region {0}")]
    MissingMemory(ResourcePath),
    #[error("Archive has no scheduler entry")]
    MissingScheduler,
    #[error("Archive does not match the layout of this machine")]
    LayoutMismatch,
    #[error("Component {path} has version {found}, expected {expected}")]
    VersionMismatch {
        path: ComponentPath,
        expected: u32,
        found: u32,
    },
    #[error("Tasks of component {0} do not match this machine")]
    TaskMismatch(ComponentPath),
    #[error("Memory region {path} has size {found}, expected {expected}")]
    RegionSizeMismatch {
        path: ResourcePath,
        expected: usize,
        found: usize,
    },
}
