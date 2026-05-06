mod codec;
mod save;
mod snapshot;

pub use codec::*;
use serde::{Deserialize, Serialize, de::DeserializeOwned};
pub use snapshot::*;

use crate::component::Component;

pub const MAGIC: [u8; 7] = *b"fluxemu";

#[derive(Debug, Serialize, Deserialize, Clone, Copy)]
pub enum CompressionFormat {
    Zlib,
}

/// Version that components use
pub type PersistanceFormatVersion = u32;

pub trait AutoSerializableComponent: Component {
    type SaveState<'a>: Serialize + ToOwned<Owned: DeserializeOwned> + 'a
    where
        Self: 'a;
    type SnapshotState<'a>: Serialize + ToOwned<Owned: DeserializeOwned> + 'a
    where
        Self: 'a;

    const VERSION: PersistanceFormatVersion;

    fn impending_snapshot_load(&mut self) {}

    fn read_save(&self) -> Self::SaveState<'_>;
    fn read_snapshot(&self) -> Self::SnapshotState<'_>;

    fn write_save(&mut self, save: <Self::SaveState<'_> as ToOwned>::Owned);
    fn write_snapshot(&mut self, snapshot: <Self::SnapshotState<'_> as ToOwned>::Owned);
}
