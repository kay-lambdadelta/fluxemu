mod id;
mod info;
mod manager;
mod store;

pub use id::*;
pub use info::*;
pub use manager::{ProgramManager, *};
use serde::{Deserialize, Serialize};
pub use store::*;

/// Identifier for the emulator to recognize a program as unique and info on it
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Manifest {
    /// Id
    pub id: ProgramId,
    /// (Usually) database derived information
    pub info: ProgramInfo,
}
