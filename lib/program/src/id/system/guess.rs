use std::{ops::RangeInclusive, path::Path};

use fluxemu_math::range::ContiguousRange;

use super::{NintendoSystem, SegaSystem, SystemId};

#[derive(Debug)]
struct MagicTableEntry {
    bytes: &'static [u8],
    offset: usize,
}

/// Magic number table
static MAGIC_TABLE: &[(SystemId, MagicTableEntry)] = &[
    (
        SystemId::Nintendo(NintendoSystem::GameBoy),
        MagicTableEntry {
            bytes: &[0xce, 0xed, 0x66, 0x66, 0xcc, 0x0d, 0x00, 0x0b],
            offset: 0x104,
        },
    ),
    (
        SystemId::Nintendo(NintendoSystem::NintendoEntertainmentSystem),
        MagicTableEntry {
            bytes: b"NES\x1a",
            offset: 0x00,
        },
    ),
    (
        SystemId::Sega(SegaSystem::Genesis),
        MagicTableEntry {
            bytes: b"SEGA GENESIS",
            offset: 0x100,
        },
    ),
    (
        SystemId::Sega(SegaSystem::Genesis),
        MagicTableEntry {
            bytes: b"SEGA MEGA DRIVE",
            offset: 0x100,
        },
    ),
    (
        SystemId::Sega(SegaSystem::MasterSystem),
        MagicTableEntry {
            bytes: b"TMR SEGA",
            offset: 0x1ff0,
        },
    ),
    (
        SystemId::Sega(SegaSystem::MasterSystem),
        MagicTableEntry {
            bytes: b"TMR SEGA",
            offset: 0x3ff0,
        },
    ),
    (
        SystemId::Sega(SegaSystem::MasterSystem),
        MagicTableEntry {
            bytes: b"TMR SEGA",
            offset: 0x7ff0,
        },
    ),
];

const GAME_BOY_COLOR_FLAG_OFFSET: usize = 0x143;

/// Guess a the system from a rom file on disk, using a variety of heuristics
pub fn guess(path: Option<&Path>, data: Option<&[u8]>) -> Option<SystemId> {
    // This goes first since a lot of roms have misleading or nonexistent magic bytes
    if let Some(path) = path
        && let Some(system) = guess_by_extension(path)
    {
        return Some(system);
    }

    if let Some(data) = data {
        for (system, entry) in MAGIC_TABLE {
            let range = RangeInclusive::from_start_and_length(entry.offset, entry.bytes.len());

            if *range.end() >= data.len() {
                continue;
            }

            if &data[range] != entry.bytes {
                continue;
            }

            if *system == SystemId::Nintendo(NintendoSystem::GameBoy)
                && data
                    .get(GAME_BOY_COLOR_FLAG_OFFSET)
                    .is_some_and(|flag| flag & 0x80 != 0)
            {
                return Some(SystemId::Nintendo(NintendoSystem::GameBoyColor));
            }

            return Some(*system);
        }
    }

    None
}

/// Try to guess the system from the file extension
fn guess_by_extension(rom: &Path) -> Option<SystemId> {
    let file_extension = rom.extension()?.to_string_lossy().to_lowercase();

    let mut systems =
        SystemId::iter().filter(|system| system.extensions().contains(&file_extension.as_str()));
    let system = systems.next()?;

    systems.next().is_none().then_some(system)
}
