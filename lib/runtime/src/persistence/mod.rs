mod save;
mod snapshot;

pub use save::*;
use serde::{Deserialize, Serialize};
pub use snapshot::*;

pub const MAGIC: [u8; 7] = *b"fluxemu";

#[derive(Debug, Serialize, Deserialize, Clone, Copy)]
pub enum CompressionFormat {
    Zlib,
}

/// Version that components use
pub type PersistanceFormatVersion = u32;
