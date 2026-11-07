use std::collections::BTreeMap;

use fluxemu_program::{ProgramId, RomId};
use rmp::Marker;
use serde::{Deserialize, Serialize};
use serde_with::{Bytes, serde_as};
use strum::FromRepr;

use crate::{ComponentPath, ResourcePath, persistence::SnapshotError};

const MAGIC: [u8; 7] = *b"fluxemu";
const FORMAT_VERSION: u32 = 0;

const FORMAT_OFFSET: usize = MAGIC.len();
const KIND_OFFSET: usize = FORMAT_OFFSET + size_of::<u32>();
pub(super) const HASH_OFFSET: usize = KIND_OFFSET + size_of::<u8>();
pub(super) const DOCUMENT_OFFSET: usize = HASH_OFFSET + blake3::OUT_LEN;

const ARCHIVE_FIELD_COUNT: u32 = 2;

#[derive(Debug, Clone, Copy, PartialEq, Eq, FromRepr)]
#[repr(u8)]
pub(crate) enum ArchiveKind {
    Save,
    Snapshot,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub(crate) enum EntryKey {
    Scheduler,
    Component(ComponentPath),
    Memory(ResourcePath),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Identity {
    pub program: Option<ProgramId>,
    pub roms: Vec<RomId>,
}

#[serde_as]
#[derive(Debug, Deserialize)]
struct Document<'a> {
    identity: Identity,
    #[serde_as(as = "BTreeMap<_, Bytes>")]
    #[serde(borrow)]
    entries: BTreeMap<EntryKey, &'a [u8]>,
}

fn write_length_placeholder(output: &mut Vec<u8>, marker: Marker) -> usize {
    output.push(marker.to_u8());

    let offset = output.len();
    output.extend_from_slice(&[0; size_of::<u32>()]);

    offset
}

fn patch_length(output: &mut [u8], offset: usize, length: u32) {
    output[offset..offset + size_of::<u32>()].copy_from_slice(&length.to_be_bytes());
}

#[derive(Debug)]
pub(crate) struct ArchiveWriter<'a> {
    output: &'a mut Vec<u8>,
    entries_length_offset: usize,
    entry_count: u32,
}

impl<'a> ArchiveWriter<'a> {
    pub fn new(
        output: &'a mut Vec<u8>,
        kind: ArchiveKind,
        identity: &Identity,
    ) -> Result<Self, SnapshotError> {
        output.clear();
        output.extend_from_slice(&MAGIC);
        output.extend_from_slice(&FORMAT_VERSION.to_le_bytes());
        output.push(kind as u8);
        output.resize(DOCUMENT_OFFSET, 0);

        let array_length_offset = write_length_placeholder(output, Marker::Array32);
        patch_length(output, array_length_offset, ARCHIVE_FIELD_COUNT);

        rmp_serde::encode::write(output, identity)?;

        let entries_length_offset = write_length_placeholder(output, Marker::Map32);

        Ok(Self {
            output,
            entries_length_offset,
            entry_count: 0,
        })
    }

    pub fn entry(
        &mut self,
        key: EntryKey,
        write: impl FnOnce(&mut Vec<u8>) -> Result<(), SnapshotError>,
    ) -> Result<(), SnapshotError> {
        rmp_serde::encode::write(self.output, &key)?;

        let length_offset = write_length_placeholder(self.output, Marker::Bin32);
        let payload_offset = self.output.len();

        write(self.output)?;

        let length = (self.output.len() - payload_offset)
            .try_into()
            .map_err(|_| SnapshotError::EntryTooLarge)?;

        patch_length(self.output, length_offset, length);

        self.entry_count += 1;

        Ok(())
    }

    pub fn finish(self) {
        patch_length(self.output, self.entries_length_offset, self.entry_count);

        let hash = blake3::hash(&self.output[DOCUMENT_OFFSET..]);
        self.output[HASH_OFFSET..DOCUMENT_OFFSET].copy_from_slice(hash.as_bytes());
    }
}

#[derive(Debug)]
pub(crate) struct ArchiveReader<'a>(Document<'a>);

impl<'a> ArchiveReader<'a> {
    pub fn open(
        bytes: &'a [u8],
        kind: ArchiveKind,
        check_hash: bool,
    ) -> Result<Self, SnapshotError> {
        if bytes.len() < DOCUMENT_OFFSET || bytes[..MAGIC.len()] != MAGIC {
            return Err(SnapshotError::NotAnArchive);
        }

        let format = u32::from_le_bytes(bytes[FORMAT_OFFSET..KIND_OFFSET].try_into().unwrap());
        if format != FORMAT_VERSION {
            return Err(SnapshotError::UnsupportedFormat(format));
        }

        if ArchiveKind::from_repr(bytes[KIND_OFFSET]) != Some(kind) {
            return Err(SnapshotError::WrongKind);
        }

        let document = &bytes[DOCUMENT_OFFSET..];

        if check_hash && blake3::hash(document) != bytes[HASH_OFFSET..DOCUMENT_OFFSET] {
            return Err(SnapshotError::Corrupt);
        }

        Ok(Self(rmp_serde::from_slice(document)?))
    }

    pub fn identity(&self) -> &Identity {
        &self.0.identity
    }

    pub fn entry_count(&self) -> usize {
        self.0.entries.len()
    }

    pub fn entry(&self, key: &EntryKey) -> Option<&'a [u8]> {
        self.0.entries.get(key).copied()
    }

    pub fn entries(&self) -> impl Iterator<Item = (&EntryKey, &'a [u8])> {
        self.0.entries.iter().map(|(key, bytes)| (key, *bytes))
    }
}
