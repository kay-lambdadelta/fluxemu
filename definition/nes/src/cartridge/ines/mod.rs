use std::{collections::HashMap, fmt::Debug, io::Cursor, ops::RangeInclusive};

use bitstream_io::{BigEndian, BitRead2, BitReader};
use expansion_device::DefaultExpansionDevice;
use fluxemu_range::ContiguousRange;
use thiserror::Error;

pub mod expansion_device;

pub const PRG_BANK_SIZE: usize = 16 * 1024;
pub const CHR_BANK_SIZE: usize = 8 * 1024;
pub const HEADER_SIZE: usize = 16;

#[derive(Error, Debug)]
pub enum ParsingError {
    #[error("Bad magic {bytes:?}")]
    BadMagic { bytes: [u8; 4] },
    #[error("Bad version {version}")]
    BadVersion { version: u8 },
    #[error("Bad console type")]
    BadConsoleType,
    #[error("Non volatile memory settings do not agree")]
    DisagreeingNonVolatileMemory,
    #[error("Not enough bytes left to be valid")]
    EarlyEOF,
    #[error("IO error")]
    Io(#[from] std::io::Error),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum TimingMode {
    Ntsc,
    Pal,
    Multi,
    Dendy,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ConsoleType {
    NintendoEntertainmentSystem,
    NintendoVsSystem,
    NintendoPlaychoice10,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum INesVersion {
    V1,
    V2 {
        console_type: ConsoleType,
        submapper: u8,
        misc_rom_count: u8,
        default_expansion_device: Option<DefaultExpansionDevice>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Mirroring {
    Vertical,
    Horizontal,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum RomType {
    Trainer,
    Prg,
    Chr,
}

#[derive(Clone, Debug)]
pub struct INes {
    pub mapper: u16,
    pub alternative_nametables: bool,
    pub non_volatile_memory: bool,
    pub mirroring: Mirroring,
    pub version: INesVersion,
    pub timing_mode: TimingMode,
    pub roms: HashMap<RomType, RangeInclusive<usize>>,
}

impl INes {
    pub fn parse(bytes: [u8; HEADER_SIZE]) -> Result<Self, ParsingError> {
        if &bytes[0..4] != b"NES\x1a" {
            return Err(ParsingError::BadMagic {
                bytes: bytes[0..4].try_into().unwrap(),
            });
        }

        let total_bits = bytes[4..].len() * u8::BITS as usize;

        let mut reader = BitReader::endian(Cursor::new(&bytes[4..]), BigEndian);

        let mut prg_bank_count: u16 = reader.read(8)?;
        let mut chr_bank_count: u16 = reader.read(8)?;

        let mut mapper: u16 = reader.read(4)?;
        let alternative_nametables = reader.read_bit()?;
        let trainer = reader.read_bit()?;
        let non_volatile_memory = reader.read_bit()?;
        let mirroring = if reader.read_bit()? {
            Mirroring::Vertical
        } else {
            Mirroring::Horizontal
        };

        mapper |= reader.read::<u16>(4)? << 4;

        let version_bits: u8 = reader.read(2)?;
        let (version, timing_mode) = match version_bits {
            0b00 => (INesVersion::V1, TimingMode::Ntsc),

            0b10 => {
                let console_type = match reader.read::<u8>(2)? {
                    0b00 => Some(ConsoleType::NintendoEntertainmentSystem),
                    0b01 => Some(ConsoleType::NintendoVsSystem),
                    0b10 => Some(ConsoleType::NintendoPlaychoice10),
                    0b11 => None,
                    _ => unreachable!(),
                };

                let submapper: u8 = reader.read(4)?;

                mapper |= reader.read::<u16>(4)? << 8;
                prg_bank_count |= reader.read::<u16>(4)? << 8;
                chr_bank_count |= reader.read::<u16>(4)? << 8;

                let prg_nvram_shift_count: u8 = reader.read(4)?;
                let prg_ram_shift_count: u8 = reader.read(4)?;

                if !non_volatile_memory && (prg_nvram_shift_count != 0 || prg_ram_shift_count != 0)
                {
                    return Err(ParsingError::DisagreeingNonVolatileMemory);
                }

                let _chr_nvram_shift_count: u8 = reader.read(4)?;
                let _chr_ram_shift_count: u8 = reader.read(4)?;

                reader.skip(6)?;

                let timing_mode = match reader.read::<u8>(2)? {
                    0b00 => TimingMode::Ntsc,
                    0b01 => TimingMode::Pal,
                    0b10 => TimingMode::Multi,
                    0b11 => TimingMode::Dendy,
                    _ => unreachable!(),
                };

                let _vs_system_type: u8 = reader.read(4)?;
                let _vs_ppu_type: u8 = reader.read(4)?;

                reader.skip(6)?;

                let misc_rom_count: u8 = reader.read(2)?;

                reader.skip(2)?;

                let default_expansion_device = DefaultExpansionDevice::new(reader.read(6)?);

                assert_eq!(
                    reader.position_in_bits()?,
                    total_bits as u64,
                    "Parser misalignment"
                );

                (
                    INesVersion::V2 {
                        console_type: console_type.ok_or(ParsingError::BadConsoleType)?,
                        submapper,
                        misc_rom_count,
                        default_expansion_device,
                    },
                    timing_mode,
                )
            }

            _ => {
                return Err(ParsingError::BadVersion {
                    version: version_bits,
                });
            }
        };

        let mut roms = HashMap::new();
        let mut cursor = HEADER_SIZE;

        if trainer {
            roms.insert(RomType::Trainer, cursor..=(cursor + 512 - 1));
            cursor += 512;
        }

        let prg_bank_size = prg_bank_count as usize * PRG_BANK_SIZE;
        roms.insert(
            RomType::Prg,
            RangeInclusive::from_start_and_length(cursor, prg_bank_size),
        );
        cursor += prg_bank_size;

        let chr_bank_size = chr_bank_count as usize * CHR_BANK_SIZE;
        roms.insert(
            RomType::Chr,
            RangeInclusive::from_start_and_length(cursor, chr_bank_size),
        );

        Ok(Self {
            mapper,
            alternative_nametables,
            non_volatile_memory,
            mirroring,
            version,
            timing_mode,
            roms,
        })
    }

    pub fn prg_bank_count(&self) -> usize {
        self.roms
            .get(&RomType::Prg)
            .map_or(1, |rom| rom.clone().count() / PRG_BANK_SIZE)
    }

    pub fn chr_bank_count(&self) -> usize {
        self.roms
            .get(&RomType::Chr)
            .map_or(1, |rom| rom.clone().count() / CHR_BANK_SIZE)
    }
}
