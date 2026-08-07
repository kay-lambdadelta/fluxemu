use std::{fmt::Debug, io::Cursor, ops::RangeInclusive};

use bitstream_io::{BigEndian, BitRead2, BitReader};
use bytes::Bytes;
use expansion_device::DefaultExpansionDevice;
use fluxemu_range::ContiguousRange;
use thiserror::Error;

pub mod expansion_device;

const PRG_BANK_SIZE: usize = 16 * 1024;
const CHR_BANK_SIZE: usize = 8 * 1024;
const HEADER_SIZE: usize = 16;
const TRAINER_SIZE: usize = 512;

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
pub enum NametableMirroring {
    Vertical,
    Horizontal,
}

#[derive(Clone, Debug)]
pub struct INes {
    pub mapper: u16,
    pub alternative_nametables: bool,
    pub non_volatile_memory: bool,
    pub mirroring: NametableMirroring,
    pub version: INesVersion,
    pub timing_mode: TimingMode,
    pub trainer: bool,
    pub chr_ram_size: usize,
    pub chr_nvram_size: usize,
    pub chr_rom_size: Option<usize>,
    pub prg_ram_size: usize,
    pub prg_rom_size: usize,
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
            NametableMirroring::Vertical
        } else {
            NametableMirroring::Horizontal
        };

        mapper |= reader.read::<u16>(4)? << 4;

        let version_bits: u8 = reader.read(2)?;
        let (version, timing_mode, chr_ram_size, chr_nvram_size, prg_ram_size) = match version_bits
        {
            0b00 => (
                INesVersion::V1,
                TimingMode::Ntsc,
                if chr_bank_count == 0 { 8 * 1024 } else { 0 },
                0,
                if non_volatile_memory { 8 * 1024 } else { 0 },
            ),

            0b10 => {
                // iNES 2.0
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

                let chr_nvram_shift_count: u8 = reader.read(4)?;
                let chr_ram_shift_count: u8 = reader.read(4)?;

                let chr_ram_size = if chr_ram_shift_count > 0 {
                    64 << chr_ram_shift_count
                } else {
                    0
                };
                let chr_nvram_size = if chr_nvram_shift_count > 0 {
                    64 << chr_nvram_shift_count
                } else {
                    0
                };

                let prg_ram_size = {
                    let nvram = if prg_nvram_shift_count > 0 {
                        64 << prg_nvram_shift_count
                    } else {
                        0
                    };
                    let ram = if prg_ram_shift_count > 0 {
                        64 << prg_ram_shift_count
                    } else {
                        0
                    };

                    nvram + ram
                };

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
                    chr_ram_size,
                    chr_nvram_size,
                    prg_ram_size,
                )
            }

            _ => {
                return Err(ParsingError::BadVersion {
                    version: version_bits,
                });
            }
        };

        let prg_rom_size = prg_bank_count as usize * PRG_BANK_SIZE;
        let chr_rom_size = if chr_bank_count != 0 {
            Some(chr_bank_count as usize * CHR_BANK_SIZE)
        } else {
            None
        };

        Ok(Self {
            mapper,
            alternative_nametables,
            non_volatile_memory,
            mirroring,
            version,
            timing_mode,
            trainer,
            prg_rom_size,
            chr_rom_size,
            chr_ram_size,
            chr_nvram_size,
            prg_ram_size,
        })
    }

    pub fn extract_prg_rom(&self, rom: &Bytes) -> Bytes {
        let cursor = HEADER_SIZE + if self.trainer { TRAINER_SIZE } else { 0 };

        rom.slice(RangeInclusive::from_start_and_length(
            cursor,
            self.prg_rom_size,
        ))
    }

    pub fn extract_chr_rom(&self, rom: &Bytes) -> Option<Bytes> {
        let cursor = HEADER_SIZE + if self.trainer { TRAINER_SIZE } else { 0 } + self.prg_rom_size;

        self.chr_rom_size.map(|chr_rom_size| {
            rom.slice(RangeInclusive::from_start_and_length(cursor, chr_rom_size))
        })
    }
}
