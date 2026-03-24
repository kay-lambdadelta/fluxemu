use std::{collections::HashMap, ops::RangeInclusive, path::Path, sync::LazyLock};

use fluxemu_range::ContiguousRange;

use super::{AtariSystem, MachineId, NintendoSystem, OtherSystem, SegaSystem};

struct MagicTableEntry {
    bytes: &'static [u8],
    offset: usize,
}

/// Magic number table
static MAGIC_TABLE: LazyLock<HashMap<MachineId, Vec<MagicTableEntry>>> = LazyLock::new(|| {
    let mut table: HashMap<MachineId, Vec<MagicTableEntry>> = HashMap::new();

    table
        .entry(MachineId::Nintendo(NintendoSystem::GameBoy))
        .or_default()
        .extend([MagicTableEntry {
            bytes: &[0xce, 0xed, 0x66, 0x66, 0xcc, 0x0d, 0x00, 0x0b],
            offset: 0x134,
        }]);

    table
        .entry(MachineId::Nintendo(
            NintendoSystem::NintendoEntertainmentSystem,
        ))
        .or_default()
        .extend([MagicTableEntry {
            bytes: b"NES\x1a",
            offset: 0x00,
        }]);

    table
        .entry(MachineId::Sega(SegaSystem::Genesis))
        .or_default()
        .extend([
            MagicTableEntry {
                bytes: b"SEGA GENESIS",
                offset: 0x100,
            },
            MagicTableEntry {
                bytes: b"SEGA MEGA DRIVE",
                offset: 0x100,
            },
        ]);

    table
        .entry(MachineId::Sega(SegaSystem::MasterSystem))
        .or_default()
        .extend([
            MagicTableEntry {
                bytes: b"TMR SEGA",
                offset: 0x1ff0,
            },
            MagicTableEntry {
                bytes: b"TMR SEGA",
                offset: 0x3ff0,
            },
            MagicTableEntry {
                bytes: b"TMR SEGA",
                offset: 0x7ff0,
            },
        ]);

    table
});

/// Guess a the system from a rom file on disk, using a variety of heuristics
pub fn guess(path: Option<&Path>, data: Option<&[u8]>) -> Option<MachineId> {
    // This goes first since a lot of roms have misleading or nonexistent magic bytes
    if let Some(path) = path
        && let Some(system) = guess_by_extension(path)
    {
        return Some(system);
    }

    if let Some(data) = data {
        for (system, entry) in MAGIC_TABLE
            .iter()
            .flat_map(|(system, entries)| entries.iter().map(|entry| (*system, entry)))
        {
            let range = RangeInclusive::from_start_and_length(entry.offset, entry.bytes.len());

            if !range.contains(&data.len()) {
                continue;
            }

            if &data[range] == entry.bytes {
                return Some(system);
            }
        }
    }

    None
}

/// Try to guess the system from the file extension
fn guess_by_extension(rom: &Path) -> Option<MachineId> {
    if let Some(file_extension) = rom
        .extension()
        .map(|ext| ext.to_string_lossy().to_lowercase())
        && let Some(system) = match file_extension.as_str() {
            "gb" => Some(MachineId::Nintendo(NintendoSystem::GameBoy)),
            "gbc" => Some(MachineId::Nintendo(NintendoSystem::GameBoyColor)),
            "gba" => Some(MachineId::Nintendo(NintendoSystem::GameBoyAdvance)),
            "nes" => Some(MachineId::Nintendo(
                NintendoSystem::NintendoEntertainmentSystem,
            )),
            "sfc" | "smc" => Some(MachineId::Nintendo(
                NintendoSystem::SuperNintendoEntertainmentSystem,
            )),
            "n64" | "z64" => Some(MachineId::Nintendo(NintendoSystem::Nintendo64)),
            "md" => Some(MachineId::Sega(SegaSystem::MasterSystem)),
            "gg" => Some(MachineId::Sega(SegaSystem::GameGear)),
            "ch8" | "c8" => Some(MachineId::Other(OtherSystem::Chip8)),
            "a26" => Some(MachineId::Atari(AtariSystem::Atari2600)),
            "a52" => Some(MachineId::Atari(AtariSystem::Atari5200)),
            "a78" => Some(MachineId::Atari(AtariSystem::Atari7800)),
            _ => None,
        }
    {
        return Some(system);
    }

    None
}
