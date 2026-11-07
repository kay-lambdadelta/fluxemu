use std::{
    cmp::Reverse,
    io::{Error, ErrorKind},
    path::{Path, PathBuf},
    time::SystemTime,
};

use serde::{Deserialize, Serialize};

use crate::save::write_atomically;

const SLOT_FILE_EXTENSIONS: [&str; 4] = ["save", "snapshot", "webp", "toml"];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SlotKind {
    Save,
    Snapshot,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SlotMetadata {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SlotInfo {
    pub slot: u8,
    pub metadata: SlotMetadata,
    pub modified: SystemTime,
}

impl SlotKind {
    fn extension(self) -> &'static str {
        match self {
            SlotKind::Save => "save",
            SlotKind::Snapshot => "snapshot",
        }
    }
}

impl SlotMetadata {
    pub fn display_name(&self) -> Option<&str> {
        self.name
            .as_deref()?
            .lines()
            .map(str::trim)
            .find(|line| !line.is_empty())
    }
}

fn metadata_path(directory: &Path, slot: u8) -> PathBuf {
    directory.join(format!("{slot}.toml"))
}

pub(crate) fn read_metadata(directory: &Path, slot: u8) -> SlotMetadata {
    let path = metadata_path(directory, slot);

    let contents = match std::fs::read_to_string(&path) {
        Ok(contents) => contents,
        Err(error) if error.kind() == ErrorKind::NotFound => return SlotMetadata::default(),
        Err(error) => {
            tracing::warn!("Could not read slot metadata {}: {}", path.display(), error);

            return SlotMetadata::default();
        }
    };

    match toml::from_str(&contents) {
        Ok(metadata) => metadata,
        Err(error) => {
            tracing::warn!(
                "Could not parse slot metadata {}: {}",
                path.display(),
                error
            );

            SlotMetadata::default()
        }
    }
}

pub(crate) fn write_metadata(
    directory: &Path,
    slot: u8,
    metadata: &SlotMetadata,
) -> std::io::Result<()> {
    let contents =
        toml::to_string(metadata).map_err(|error| Error::new(ErrorKind::InvalidData, error))?;

    write_atomically(&metadata_path(directory, slot), contents.as_bytes())
}

pub(crate) fn list_slots(directory: &Path, kind: SlotKind) -> Vec<SlotInfo> {
    let Ok(entries) = std::fs::read_dir(directory) else {
        return Vec::new();
    };

    let mut slots: Vec<_> = entries
        .flatten()
        .filter_map(|entry| {
            let path = entry.path();

            if path.extension()?.to_str()? != kind.extension() {
                return None;
            }

            let slot = path.file_stem()?.to_str()?.parse().ok()?;

            Some(SlotInfo {
                slot,
                metadata: read_metadata(directory, slot),
                modified: entry.metadata().ok()?.modified().ok()?,
            })
        })
        .collect();

    slots.sort_unstable_by_key(|info| (Reverse(info.modified), info.slot));

    slots
}

pub(crate) fn next_free_slot(directory: &Path, kind: SlotKind) -> Option<u8> {
    let used: Vec<_> = list_slots(directory, kind)
        .into_iter()
        .map(|info| info.slot)
        .collect();

    (0..=u8::MAX).find(|slot| !used.contains(slot))
}

pub(crate) fn delete_slot(directory: &Path, slot: u8) -> std::io::Result<()> {
    for extension in SLOT_FILE_EXTENSIONS {
        match std::fs::remove_file(directory.join(format!("{slot}.{extension}"))) {
            Ok(()) => {}
            Err(error) if error.kind() == ErrorKind::NotFound => {}
            Err(error) => return Err(error),
        }
    }

    Ok(())
}
