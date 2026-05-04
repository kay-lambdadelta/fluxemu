mod codec;
mod save;

pub use codec::*;
pub use save::*;

use serde::{Deserialize, Serialize, de::DeserializeOwned};

use crate::component::Component;

pub const MAGIC: [u8; 7] = *b"fluxemu";

#[derive(Debug, Serialize, Deserialize, Clone, Copy)]
pub enum CompressionFormat {
    Zlib,
}

/// Version that components use
pub type PersistanceFormatVersion = u32;

pub trait AutoSerializableComponent: Component {
    type SaveState<'a>: Serialize + DeserializeOwned + 'a;
    type SnapshotState<'a>: Serialize + DeserializeOwned + 'a;

    fn impending_snapshot(&mut self) {}

    fn read_save(&self) -> Self::SaveState<'_>;
    fn read_snapshot(&self) -> Self::SnapshotState<'_>;

    fn write_save(&mut self, save: Self::SaveState<'_>);
    fn write_snapshot(&mut self, snapshot: Self::SnapshotState<'_>);
}
