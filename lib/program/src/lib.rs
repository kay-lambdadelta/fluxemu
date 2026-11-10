mod cache;
mod encoding;
mod id;
mod info;
mod manager;
mod store;

use std::collections::BTreeMap;

pub use cache::DigestCache;
pub use encoding::DATABASE_VERSION;
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
    pub variant: Variant,
    pub files: BTreeMap<String, FileMatch>,
}

impl Manifest {
    pub fn file(&self, path: &str) -> Option<(&RomFile, &FileMatch)> {
        Some((self.variant.files().get(path)?, self.files.get(path)?))
    }
}
