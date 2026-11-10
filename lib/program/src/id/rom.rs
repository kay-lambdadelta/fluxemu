use std::{
    fmt::Display,
    io::{BufReader, Read, Write},
    str::FromStr,
};

use data_encoding::HEXLOWER_PERMISSIVE;
use md5::Md5;
use serde::{Deserialize, Serialize};
use serde_with::{Bytes, serde_as};
use sha1::{Digest, Sha1};
use sha2::Sha256;
use strum::{Display, EnumDiscriminants, EnumString};
use thiserror::Error;

/// Hash of a ROM, serves as its identification within emulator (this is a BLAKE3 hash)
///
/// This is the name used within the ROM store, and is used as the number one unique identity for the canonical bytes of a ROM
#[serde_as]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct RomId(#[serde_as(as = "Bytes")] pub [u8; 32]);

impl RomId {
    pub fn new(mut read: impl Read) -> Result<Self, std::io::Error> {
        let mut hasher = blake3::Hasher::new();
        std::io::copy(&mut read, &mut hasher)?;

        Ok(Self(hasher.finalize().into()))
    }
}

impl Display for RomId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", HEXLOWER_PERMISSIVE.encode(&self.0))
    }
}

#[allow(missing_docs)]
#[derive(Debug, Error)]
pub enum HashParsingError {
    #[error("{0}")]
    DataEncoding(#[from] data_encoding::DecodeError),
    #[error("Hash did not meet the requirements for this function")]
    InvalidHash,
    #[error("Unknown hash algorithm {0:?}")]
    UnknownAlgorithm(String),
    #[error("Missing hash algorithm prefix")]
    MissingAlgorithm,
    #[error("More than one {0} hash")]
    DuplicateAlgorithm(RomHashAlgorithm),
}

impl FromStr for RomId {
    type Err = HashParsingError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Self(
            HEXLOWER_PERMISSIVE
                .decode(s.as_bytes())?
                .try_into()
                .map_err(|_| HashParsingError::InvalidHash)?,
        ))
    }
}

/// These are like our [`RomId`], but these are the identifiers dats have a tendency to use
#[serde_as]
#[derive(Debug, Clone, Copy, EnumDiscriminants, Serialize, Deserialize)]
#[strum_discriminants(
    name(RomHashAlgorithm),
    derive(PartialOrd, Ord, Hash, Display, EnumString),
    strum(serialize_all = "lowercase")
)]
pub enum RomHash {
    Crc32(u32),
    Md5(#[serde_as(as = "Bytes")] [u8; 16]),
    Sha1(#[serde_as(as = "Bytes")] [u8; 20]),
    Sha256(#[serde_as(as = "Bytes")] [u8; 32]),
}

impl RomHash {
    pub fn algorithm(&self) -> RomHashAlgorithm {
        self.into()
    }

    pub fn digest_bytes(&self) -> Vec<u8> {
        match self {
            RomHash::Crc32(digest) => digest.to_be_bytes().to_vec(),
            RomHash::Md5(digest) => digest.to_vec(),
            RomHash::Sha1(digest) => digest.to_vec(),
            RomHash::Sha256(digest) => digest.to_vec(),
        }
    }

    pub fn from_digest_bytes(
        algorithm: RomHashAlgorithm,
        bytes: &[u8],
    ) -> Result<Self, HashParsingError> {
        let invalid = |_| HashParsingError::InvalidHash;

        Ok(match algorithm {
            RomHashAlgorithm::Crc32 => {
                RomHash::Crc32(u32::from_be_bytes(bytes.try_into().map_err(invalid)?))
            }
            RomHashAlgorithm::Md5 => RomHash::Md5(bytes.try_into().map_err(invalid)?),
            RomHashAlgorithm::Sha1 => RomHash::Sha1(bytes.try_into().map_err(invalid)?),
            RomHashAlgorithm::Sha256 => RomHash::Sha256(bytes.try_into().map_err(invalid)?),
        })
    }
}

impl Display for RomHash {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}:{}",
            self.algorithm(),
            HEXLOWER_PERMISSIVE.encode(&self.digest_bytes())
        )
    }
}

impl FromStr for RomHash {
    type Err = HashParsingError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let (algorithm, digest) = s
            .split_once(':')
            .ok_or(HashParsingError::MissingAlgorithm)?;
        let algorithm = algorithm
            .parse()
            .map_err(|_| HashParsingError::UnknownAlgorithm(algorithm.to_string()))?;
        let digest = HEXLOWER_PERMISSIVE.decode(digest.as_bytes())?;

        Self::from_digest_bytes(algorithm, &digest)
    }
}

#[serde_as]
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
pub struct RomHashes {
    pub crc32: Option<u32>,
    #[serde_as(as = "Option<Bytes>")]
    pub md5: Option<[u8; 16]>,
    #[serde_as(as = "Option<Bytes>")]
    pub sha1: Option<[u8; 20]>,
    #[serde_as(as = "Option<Bytes>")]
    pub sha256: Option<[u8; 32]>,
}

impl RomHashes {
    pub fn iter(&self) -> impl Iterator<Item = RomHash> {
        [
            self.crc32.map(RomHash::Crc32),
            self.md5.map(RomHash::Md5),
            self.sha1.map(RomHash::Sha1),
            self.sha256.map(RomHash::Sha256),
        ]
        .into_iter()
        .flatten()
    }

    pub fn insert(&mut self, hash: RomHash) -> Result<(), HashParsingError> {
        let duplicate = match hash {
            RomHash::Crc32(digest) => self.crc32.replace(digest).is_some(),
            RomHash::Md5(digest) => self.md5.replace(digest).is_some(),
            RomHash::Sha1(digest) => self.sha1.replace(digest).is_some(),
            RomHash::Sha256(digest) => self.sha256.replace(digest).is_some(),
        };

        if duplicate {
            return Err(HashParsingError::DuplicateAlgorithm(hash.algorithm()));
        }

        Ok(())
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum MatchStrength {
    Weak,
    Strong(u8),
    Canonical,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileDigest {
    pub id: RomId,
    pub size: u64,
    pub hashes: RomHashes,
}

impl FileDigest {
    pub fn new(read: impl Read) -> Result<Self, std::io::Error> {
        let mut hasher = FileHasher::default();
        let size = std::io::copy(&mut BufReader::with_capacity(64 * 1024, read), &mut hasher)?;

        Ok(Self {
            id: RomId(hasher.blake3.finalize().into()),
            size,
            hashes: RomHashes {
                crc32: Some(hasher.crc32.finalize()),
                md5: Some(hasher.md5.finalize().into()),
                sha1: Some(hasher.sha1.finalize().into()),
                sha256: Some(hasher.sha256.finalize().into()),
            },
        })
    }
}

#[derive(Default)]
struct FileHasher {
    crc32: crc32fast::Hasher,
    md5: Md5,
    sha1: Sha1,
    sha256: Sha256,
    blake3: blake3::Hasher,
}

impl Write for FileHasher {
    fn write(&mut self, chunk: &[u8]) -> std::io::Result<usize> {
        self.crc32.update(chunk);
        self.md5.update(chunk);
        self.sha1.update(chunk);
        self.sha256.update(chunk);
        self.blake3.update(chunk);

        Ok(chunk.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}
