use std::{
    io::{BufReader, Read, Seek, SeekFrom, Write},
    ops::RangeInclusive,
};

use crc32fast::Hasher;
use fluxemu_math::range::ContiguousRange;
use thiserror::Error;

use crate::rom::patch::PatchFormat;

#[derive(Debug)]
pub struct Ups;

impl PatchFormat for Ups {
    type Error = Error;

    const EXTENSION: &str = "ups";

    fn apply(
        mut input: impl Read + Seek,
        mut output: impl Read + Write + Seek,
        mut patch: impl Read + Seek,
    ) -> Result<(), Self::Error> {
        let mut magic = [0; _];
        patch.read_exact(&mut magic)?;
        if magic != MAGIC {
            return Err(Error::InvalidMagic);
        }

        let source_size = read_vlq(&mut patch)?;
        let target_size = read_vlq(&mut patch)?;

        {
            let file_size = stream_len(&mut input)?;
            if source_size != file_size {
                return Err(Error::Io(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "source size does not match file size",
                )));
            }
        }

        let patch_len = stream_len(&mut patch)?;

        let footer_start = patch_len.checked_sub(3 * CRC32_SIZE).ok_or_else(|| {
            Error::Io(std::io::Error::new(
                std::io::ErrorKind::UnexpectedEof,
                "patch file too short to contain a footer",
            ))
        })?;
        let footer = read_footer(&mut patch, footer_start)?;

        let actual_patch_crc = calculate_crc32(
            &mut patch,
            RangeInclusive::from_start_and_length(0, patch_len - CRC32_SIZE),
        )?;
        if actual_patch_crc != footer.patch_crc {
            return Err(Error::PatchCrcMismatch {
                expected: footer.patch_crc,
                actual: actual_patch_crc,
            });
        }

        let actual_source_crc = calculate_crc32(
            &mut input,
            RangeInclusive::from_start_and_length(0, source_size),
        )?;
        if actual_source_crc != footer.source_crc {
            return Err(Error::SourceCrcMismatch {
                expected: footer.source_crc,
                actual: actual_source_crc,
            });
        }

        let mut position: u64 = 0;

        while patch.stream_position()? < footer_start {
            let relative_offset = read_vlq(&mut patch)?;
            let new_position = position + relative_offset;

            copy_range(&mut input, &mut output, position, new_position, source_size)?;
            position = new_position;

            loop {
                let mut patch_byte = 0;
                patch.read_exact(std::array::from_mut(&mut patch_byte))?;

                if position >= target_size {
                    if patch_byte != 0 {
                        return Err(Error::Io(std::io::Error::new(
                            std::io::ErrorKind::InvalidData,
                            "patch hunk writes past end of target",
                        )));
                    }

                    position += 1;
                    break;
                }

                let source_byte = if position < source_size {
                    let mut byte = 0;

                    input.read_exact(std::array::from_mut(&mut byte))?;

                    byte
                } else {
                    0
                };

                output.write_all(&[source_byte ^ patch_byte])?;
                position += 1;

                if patch_byte == 0 {
                    break;
                }
            }
        }

        copy_range(&mut input, &mut output, position, target_size, source_size)?;

        output.flush()?;

        // Read the final CRC32 for verification
        let output = BufReader::new(output);
        let actual_target_crc = calculate_crc32(
            output,
            RangeInclusive::from_start_and_length(0, target_size),
        )?;

        if actual_target_crc != footer.target_crc {
            return Err(Error::TargetCrcMismatch {
                expected: footer.target_crc,
                actual: actual_target_crc,
            });
        }

        Ok(())
    }
}

const CRC32_SIZE: u64 = size_of::<u32>() as u64;
const MAGIC: [u8; 4] = *b"UPS1";

struct Footer {
    pub source_crc: u32,
    pub target_crc: u32,
    pub patch_crc: u32,
}

#[derive(Debug, Error)]
pub enum Error {
    #[error("invalid magic")]
    InvalidMagic,
    #[error("IO error {0}")]
    Io(#[from] std::io::Error),
    #[error("source file CRC32 mismatch: expected {expected:08x}, got {actual:08x}")]
    SourceCrcMismatch { expected: u32, actual: u32 },
    #[error("target file CRC32 mismatch: expected {expected:08x}, got {actual:08x}")]
    TargetCrcMismatch { expected: u32, actual: u32 },
    #[error("patch file CRC32 mismatch: expected {expected:08x}, got {actual:08x}")]
    PatchCrcMismatch { expected: u32, actual: u32 },
}

#[inline]
fn copy_range(
    mut input: impl Read + Seek,
    mut output: impl Write + Seek,
    start: u64,
    end: u64,
    source_size: u64,
) -> std::io::Result<()> {
    if start >= end {
        return Ok(());
    }

    output.seek(SeekFrom::Start(start))?;

    let mut position = start;

    if position < source_size {
        input.seek(SeekFrom::Start(position))?;

        let copy_len = end.min(source_size) - position;
        std::io::copy(&mut input.take(copy_len), &mut output)?;

        position += copy_len;
    }

    if position < end {
        std::io::copy(
            &mut Read::take(std::io::repeat(0), end - position),
            &mut output,
        )?;
    }

    Ok(())
}

#[inline]
fn read_vlq(mut reader: impl Read) -> std::io::Result<u64> {
    let mut result = 0;
    let mut shift = 1;

    loop {
        let mut byte = 0;
        reader.read_exact(std::array::from_mut(&mut byte))?;

        result += ((byte & 0b01111111) as u64) * shift;

        if byte & 0b10000000 != 0 {
            // Final byte
            break;
        }

        shift <<= 7;
        result += shift;
    }

    Ok(result)
}

#[inline]
fn stream_len(mut stream: impl Seek) -> std::io::Result<u64> {
    let current = stream.stream_position()?;
    let len = stream.seek(SeekFrom::End(0))?;
    stream.seek(SeekFrom::Start(current))?;

    Ok(len)
}

#[inline]
fn read_footer(mut patch: impl Read + Seek, footer_start: u64) -> std::io::Result<Footer> {
    let saved_pos = patch.stream_position()?;
    patch.seek(SeekFrom::Start(footer_start))?;

    let mut buffer = [0; 12];
    patch.read_exact(&mut buffer)?;
    patch.seek(SeekFrom::Start(saved_pos))?;

    let source_crc = u32::from_le_bytes(buffer[0..4].try_into().unwrap());
    let target_crc = u32::from_le_bytes(buffer[4..8].try_into().unwrap());
    let patch_crc = u32::from_le_bytes(buffer[8..12].try_into().unwrap());

    Ok(Footer {
        source_crc,
        target_crc,
        patch_crc,
    })
}

#[inline]
fn calculate_crc32(
    mut reader: impl Read + Seek,
    range: RangeInclusive<u64>,
) -> std::io::Result<u32> {
    let saved_position = reader.stream_position()?;
    reader.seek(SeekFrom::Start(*range.start()))?;

    let mut hasher = Hasher::new();
    let mut buffer = [0; 64 * 1024];
    let mut remaining = range.len();

    while remaining > 0 {
        let chunk = remaining.min(buffer.len() as u64) as usize;

        reader.read_exact(&mut buffer[..chunk])?;
        hasher.update(&buffer[..chunk]);

        remaining -= chunk as u64;
    }

    reader.seek(SeekFrom::Start(saved_position))?;

    Ok(hasher.finalize())
}
