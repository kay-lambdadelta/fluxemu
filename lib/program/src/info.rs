use std::collections::{BTreeMap, BTreeSet};

use bytes::Bytes;
use fluxemu_locale::{Iso639Alpha3, Iso3166Alpha2};
use pathfinding::{kuhn_munkres::kuhn_munkres, matrix::Matrix};
use serde::{Deserialize, Serialize};
use strum::IntoStaticStr;

use crate::{FileDigest, MatchStrength, ProgramId, RomHash, RomHashes, RomId};

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
/// Information about a program, for the database
pub enum ProgramInfo {
    /// Version 0
    #[serde(rename = "0")]
    V0 {
        title: Title,
        /// Identifiable names of the program
        ///
        /// Preferably these will be the names associated with the below languages, in their original script
        alternative_titles: BTreeSet<Title>,
        regions: BTreeSet<Region>,
        /// The language this program is associated with
        ///
        /// Note that this is the languages a coherent title supports
        ///
        /// If alternate files are required a different database entry is required
        languages: BTreeSet<Iso639Alpha3>,
        /// The version or revision of the program
        revision: Option<String>,
        kind: ProgramKind,
        flags: BTreeSet<ProgramFlag>,
        video_standards: BTreeSet<VideoStandard>,
        serials: BTreeSet<String>,
    },
}

impl ProgramInfo {
    pub fn title(&self) -> &Title {
        match self {
            ProgramInfo::V0 { title, .. } => title,
        }
    }

    pub fn alternative_titles(&self) -> &BTreeSet<Title> {
        match self {
            ProgramInfo::V0 {
                alternative_titles, ..
            } => alternative_titles,
        }
    }

    pub fn regions(&self) -> &BTreeSet<Region> {
        match self {
            ProgramInfo::V0 { regions, .. } => regions,
        }
    }

    pub fn languages(&self) -> &BTreeSet<Iso639Alpha3> {
        match self {
            ProgramInfo::V0 { languages, .. } => languages,
        }
    }

    pub fn kind(&self) -> ProgramKind {
        match self {
            ProgramInfo::V0 { kind, .. } => *kind,
        }
    }

    pub fn flags(&self) -> &BTreeSet<ProgramFlag> {
        match self {
            ProgramInfo::V0 { flags, .. } => flags,
        }
    }

    pub fn serials(&self) -> &BTreeSet<String> {
        match self {
            ProgramInfo::V0 { serials, .. } => serials,
        }
    }

    pub fn revision(&self) -> Option<&str> {
        match self {
            ProgramInfo::V0 { revision, .. } => revision.as_deref(),
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Title {
    pub text: String,
    pub language: Option<Iso639Alpha3>,
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Region {
    Country(Iso3166Alpha2),
    World,
    Europe,
    Asia,
    LatinAmerica,
    Scandinavia,
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum VideoStandard {
    Ntsc,
    Pal,
}

#[derive(
    Serialize,
    Deserialize,
    Debug,
    Default,
    Clone,
    Copy,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    IntoStaticStr,
)]
#[strum(serialize_all = "snake_case")]
pub enum ProgramKind {
    #[default]
    Game,
    Application,
    Demo,
    Educational,
    Firmware,
    AddOn,
    Audio,
    Video,
    Other,
}

#[derive(
    Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, IntoStaticStr,
)]
#[strum(serialize_all = "snake_case")]
pub enum ProgramFlag {
    Beta,
    Prototype,
    Sample,
    Unlicensed,
    Pirate,
    Homebrew,
    Hack,
    Translation,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum Variant {
    #[serde(rename = "0")]
    V0 {
        /// Paths are unix like
        ///
        /// This is the virtual filesystem that composes a program, and is the lookup key the emulator uses
        /// and the exporter exports with
        files: BTreeMap<String, RomFile>,
    },
}

impl Variant {
    pub fn files(&self) -> &BTreeMap<String, RomFile> {
        match self {
            Variant::V0 { files } => files,
        }
    }

    pub fn files_mut(&mut self) -> &mut BTreeMap<String, RomFile> {
        match self {
            Variant::V0 { files } => files,
        }
    }

    pub fn rom_ids(&self) -> impl Iterator<Item = RomId> {
        self.files().values().filter_map(|file| file.rom_id)
    }

    pub fn search_hashes(&self) -> impl Iterator<Item = RomHash> {
        self.files().values().flat_map(RomFile::search_hashes)
    }

    pub fn dump_status(&self) -> DumpStatus {
        self.files()
            .values()
            .map(|file| file.status)
            .min()
            .unwrap_or_default()
    }

    // TODO: Implement this in nalgebra instead of being lazy and using `pathfinding`
    pub fn resolve(&self, digests: &[FileDigest]) -> Option<VariantMatch> {
        let files: Vec<_> = self.files().iter().collect();

        if files.is_empty() || files.len() > digests.len() {
            return None;
        }

        let matches = Matrix::from_fn(files.len(), digests.len(), |(file_index, digest_index)| {
            files[file_index].1.compare(&digests[digest_index])
        });

        let mut ranks: Vec<_> = matches
            .values()
            .flatten()
            .map(|file_match| (file_match.strength, file_match.form))
            .collect();

        ranks.sort();
        ranks.dedup();

        let mut thresholds: Vec<_> = ranks.iter().map(|(strength, _)| *strength).collect();
        thresholds.dedup();

        let forbidden = -((files.len() * ranks.len()) as i64 + 1);

        thresholds.into_iter().rev().find_map(|threshold| {
            let weights = matches.clone().map(|file_match| match file_match {
                Some(file_match) if file_match.strength >= threshold => {
                    ranks
                        .binary_search(&(file_match.strength, file_match.form))
                        .unwrap() as i64
                        + 1
                }
                _ => forbidden,
            });
            let (_, assignment) = kuhn_munkres(&weights);

            let files: BTreeMap<_, _> = files
                .iter()
                .zip(assignment)
                .enumerate()
                .map(|(file_index, ((path, _), digest_index))| {
                    let file_match = matches[(file_index, digest_index)]
                        .filter(|file_match| file_match.strength >= threshold)?;

                    Some(((*path).clone(), file_match))
                })
                .collect::<Option<_>>()?;

            let strength = files
                .values()
                .map(|file_match| file_match.strength)
                .min()
                .unwrap();

            Some(VariantMatch { strength, files })
        })
    }
}

#[derive(Debug, Clone)]
pub struct VariantMatch {
    pub strength: MatchStrength,
    pub files: BTreeMap<String, FileMatch>,
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq)]
pub struct FileMatch {
    pub rom_id: RomId,
    pub strength: MatchStrength,
    pub form: RomForm,
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum RomForm {
    WithNonCanonicalHeader,
    Canonical,
}

/// Stored ROMs are canonical, they have their header stripped off if it's a scene header (like iNES)
///
/// Most attributes here therefore describe the ROM with its header off
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct RomFile {
    pub size: u64,
    pub rom_id: Option<RomId>,
    pub hashes: RomHashes,
    pub status: DumpStatus,
    pub non_canonical_header: Option<NonCanonicalHeader>,
}

impl RomFile {
    pub const MINIMUM_WEAK_INDICATORS: usize = 2;

    pub fn search_hashes(&self) -> impl Iterator<Item = RomHash> {
        self.hashes.iter().chain(
            self.non_canonical_header
                .iter()
                .flat_map(|header| header.hashes_with_header.iter()),
        )
    }

    pub fn header_length(&self) -> usize {
        self.non_canonical_header
            .as_ref()
            .map_or(0, |header| header.bytes.len())
    }

    pub fn missing_header(&self, form: RomForm) -> Option<&Bytes> {
        match form {
            RomForm::Canonical => self
                .non_canonical_header
                .as_ref()
                .map(|header| &header.bytes),
            RomForm::WithNonCanonicalHeader => None,
        }
    }

    pub fn compare(&self, digest: &FileDigest) -> Option<FileMatch> {
        let file_match = |strength, form| FileMatch {
            rom_id: digest.id,
            strength,
            form,
        };

        let canonical = match self.rom_id {
            Some(rom_id) => (rom_id == digest.id).then_some(MatchStrength::Canonical),
            None => compare_hashes(self.size, &self.hashes, digest),
        }
        .map(|strength| file_match(strength, RomForm::Canonical));

        let with_header = self.non_canonical_header.as_ref().and_then(|header| {
            compare_hashes(
                self.size + header.bytes.len() as u64,
                &header.hashes_with_header,
                digest,
            )
            .map(|strength| file_match(strength, RomForm::WithNonCanonicalHeader))
        });

        canonical
            .into_iter()
            .chain(with_header)
            .max_by_key(|file_match| (file_match.strength, file_match.form))
    }
}

fn compare_hashes(size: u64, hashes: &RomHashes, digest: &FileDigest) -> Option<MatchStrength> {
    #[inline]
    fn agrees<T: PartialEq>(left: Option<T>, right: Option<T>) -> Option<bool> {
        Some(left? == right?)
    }

    fn count_agreeing(indicators: &[Option<bool>]) -> usize {
        indicators
            .iter()
            .filter(|agreement| **agreement == Some(true))
            .count()
    }

    let other = &digest.hashes;

    // These criteria are near worthless
    let weak = [
        agrees(Some(size), Some(digest.size)),
        agrees(hashes.crc32, other.crc32),
    ];

    // These criteria are strong enough to confirm identification
    let strong = [
        agrees(hashes.md5, other.md5),
        agrees(hashes.sha1, other.sha1),
        agrees(hashes.sha256, other.sha256),
    ];

    if weak.contains(&Some(false)) || strong.contains(&Some(false)) {
        return None;
    }

    match count_agreeing(&strong) {
        0 if count_agreeing(&weak) >= RomFile::MINIMUM_WEAK_INDICATORS => Some(MatchStrength::Weak),
        0 => None,
        agreeing => Some(MatchStrength::Strong(agreeing as u8)),
    }
}

impl From<&FileDigest> for RomFile {
    fn from(digest: &FileDigest) -> Self {
        Self {
            size: digest.size,
            rom_id: Some(digest.id),
            hashes: digest.hashes,
            status: DumpStatus::Unverified,
            non_canonical_header: None,
        }
    }
}

/// These are pretty much just lifted from No-Intro
#[derive(
    Serialize, Deserialize, Debug, Default, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash,
)]
pub enum DumpStatus {
    NoDump,
    Bad,
    Unverified,
    /// NOTE: This seems to be the default status for items in No-Intro
    #[default]
    Good,
    Verified,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct NonCanonicalHeader {
    pub bytes: Bytes,
    pub hashes_with_header: RomHashes,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Derivative {
    pub program: ProgramId,
    pub kind: DerivativeKind,
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum DerivativeKind {
    Translation,
    Hack,
    Fix,
    Trainer,
    Patch,
}
