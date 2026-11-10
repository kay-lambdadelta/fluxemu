use std::{
    collections::{BTreeMap, BTreeSet, HashMap},
    error::Error,
    io::BufRead,
    str::FromStr,
    sync::LazyLock,
};

use bytes::Bytes;
use data_encoding::HEXLOWER_PERMISSIVE;
use fluxemu_database::ProgramDatabase;
use fluxemu_locale::{Iso639Alpha2, Iso639Alpha3, Iso3166Alpha2};
use fluxemu_program::{
    DumpStatus, NonCanonicalHeader, ProgramFlag, ProgramId, ProgramInfo, ProgramKind, Region,
    RomFile, RomHash, RomHashes, SystemId, Title, Variant,
};
use pathfinding::undirected::connected_components::components;
use serde::Deserialize;
use serde_with::{DefaultOnError, DisplayFromStr, serde_as};

mod import;

pub use import::import;

#[derive(Debug, Deserialize)]
pub struct Datafile {
    pub header: Header,
    #[serde(alias = "machine", default)]
    pub game: Vec<Game>,
}

#[derive(Debug, Deserialize)]
pub struct Header {
    pub name: String,
}

#[derive(Debug, Deserialize)]
pub struct Game {
    #[serde(rename = "@name")]
    pub name: String,
    #[serde(rename = "@id")]
    pub id: Option<String>,
    #[serde(rename = "@cloneofid")]
    pub clone_of_id: Option<String>,
    #[serde(default)]
    pub category: Vec<String>,
    #[serde(default)]
    pub rom: Vec<Rom>,
}

#[serde_as]
#[derive(Debug, Deserialize)]
pub struct Rom {
    #[serde(rename = "@name")]
    pub path: String,
    #[serde(rename = "@size")]
    #[serde_as(as = "DefaultOnError<Option<DisplayFromStr>>")]
    pub size: Option<u64>,
    #[serde(rename = "@crc")]
    pub crc32: Option<String>,
    #[serde(rename = "@md5")]
    pub md5: Option<String>,
    #[serde(rename = "@sha1")]
    pub sha1: Option<String>,
    #[serde(rename = "@sha256")]
    pub sha256: Option<String>,
    #[serde(rename = "@status")]
    pub status: Option<String>,
    #[serde(rename = "@header")]
    pub header: Option<String>,
    #[serde(rename = "@serial")]
    pub serial: Option<String>,
}

impl Rom {
    fn hashes(&self) -> Result<RomHashes, Box<dyn Error + Send + Sync>> {
        let mut hashes = RomHashes::default();

        for (algorithm, digest) in [
            ("crc32", &self.crc32),
            ("md5", &self.md5),
            ("sha1", &self.sha1),
            ("sha256", &self.sha256),
        ] {
            if let Some(digest) = digest {
                hashes.insert(RomHash::from_str(&format!("{algorithm}:{digest}"))?)?;
            }
        }

        Ok(hashes)
    }

    fn status(&self) -> DumpStatus {
        match self.status.as_deref() {
            Some("verified") => DumpStatus::Verified,
            Some("baddump") => DumpStatus::Bad,
            Some("nodump") => DumpStatus::NoDump,
            Some("good") | None => DumpStatus::Good,
            Some(_) => DumpStatus::Unverified,
        }
    }

    fn header(&self) -> Option<Bytes> {
        let header = self.header.as_ref()?;

        let digits: String = header.split_whitespace().collect();

        match HEXLOWER_PERMISSIVE.decode(digits.as_bytes()) {
            Ok(bytes) => Some(Bytes::from(bytes)),
            Err(_) => {
                tracing::warn!(
                    "Ignoring header {:?} on ROM {}, it is not hex bytes",
                    header,
                    self.path
                );

                None
            }
        }
    }
}

pub fn parse(file: impl BufRead) -> Result<Datafile, Box<dyn Error + Send + Sync>> {
    Ok(quick_xml::de::from_reader(file)?)
}

#[derive(Default)]
struct PendingProgram {
    info: Option<ProgramInfo>,
    variants: BTreeMap<String, BTreeMap<String, RomFile>>,
    serials: BTreeSet<String>,
}

impl PendingProgram {
    fn into_variants(self) -> Vec<BTreeMap<String, RomFile>> {
        let (mut headered, mut headerless): (Vec<_>, Vec<_>) =
            self.variants.into_values().partition(|files| {
                files
                    .values()
                    .any(|file| file.non_canonical_header.is_some())
            });

        for headered_files in &mut headered {
            let Some((index, pairing)) =
                headerless
                    .iter()
                    .enumerate()
                    .find_map(|(index, headerless_files)| {
                        Some((index, header_pairing(headered_files, headerless_files)?))
                    })
            else {
                continue;
            };

            let headerless_files = headerless.remove(index);

            for (headered_path, headerless_path) in pairing {
                let headerless_file = &headerless_files[&headerless_path];
                let headered_file = headered_files.get_mut(&headered_path).unwrap();

                if headered_file.non_canonical_header.is_some() {
                    headered_file.hashes = headerless_file.hashes;
                }
                headered_file.status = headered_file.status.min(headerless_file.status);
            }
        }

        headered.into_iter().chain(headerless).collect()
    }
}

fn header_pairing(
    headered: &BTreeMap<String, RomFile>,
    headerless: &BTreeMap<String, RomFile>,
) -> Option<Vec<(String, String)>> {
    if headered.len() != headerless.len() {
        return None;
    }

    let mut paired = BTreeSet::new();

    headered
        .iter()
        .map(|(headered_path, headered_file)| {
            let (headerless_path, _) = headerless.iter().find(|(path, file)| {
                file.non_canonical_header.is_none()
                    && file.size == headered_file.size
                    && stem(path) == stem(headered_path)
                    && !paired.contains(path.as_str())
            })?;
            paired.insert(headerless_path.clone());

            Some((headered_path.clone(), headerless_path.clone()))
        })
        .collect()
}

fn stem(path: &str) -> &str {
    path.rsplit_once('.').map_or(path, |(stem, _)| stem)
}

pub fn add_to_database(
    program_database: &ProgramDatabase,
    data_files: impl IntoIterator<Item = (SystemId, Datafile)>,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    let mut data_files: Vec<_> = data_files.into_iter().collect();
    data_files.sort_by(|(_, left), (_, right)| left.header.name.cmp(&right.header.name));

    let mut programs: BTreeMap<_, PendingProgram> = BTreeMap::new();
    let mut clone_groups: BTreeMap<_, Vec<_>> = BTreeMap::new();

    for (system, data_file) in data_files {
        tracing::debug!(
            "Found {} entries in logiqx database for the system {}",
            data_file.game.len(),
            system
        );

        let data_file_name = data_file.header.name;

        for game in data_file.game {
            if game.rom.is_empty() {
                continue;
            }

            let program_id = ProgramId {
                system,
                main_name: game.name.clone(),
            };

            if let Some(group) = game.clone_of_id.as_ref().or(game.id.as_ref()) {
                clone_groups
                    .entry((data_file_name.clone(), group.clone()))
                    .or_default()
                    .push(program_id.clone());
            }

            let pending = programs.entry(program_id).or_default();

            if pending.info.is_none() {
                pending.info = Some(program_info(&game));
            }

            for rom in &game.rom {
                if let Err(err) = merge_rom(pending, &data_file_name, rom) {
                    tracing::warn!("Skipping ROM {} in {}: {}", rom.path, game.name, err);
                }
            }
        }
    }

    let writer = program_database.write()?;

    for (program_id, pending) in programs {
        let Some(mut info) = pending.info.clone() else {
            continue;
        };

        let ProgramInfo::V0 { serials, .. } = &mut info;
        serials.extend(pending.serials.iter().cloned());

        writer.insert_program(&program_id, &info)?;
        writer.clear_variants(&program_id)?;

        for files in pending.into_variants() {
            writer.insert_variant(&program_id, &Variant::V0 { files })?;
        }
    }

    let clone_groups: Vec<Vec<_>> = clone_groups.into_values().collect();

    for members in components(&clone_groups) {
        if members.len() > 1 {
            writer.set_clone_group(members)?;
        }
    }

    writer.commit()?;

    Ok(())
}

fn merge_rom(
    pending: &mut PendingProgram,
    data_file_name: &str,
    rom: &Rom,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    if let Some(serial) = &rom.serial {
        pending.serials.insert(serial.clone());
    }

    let hashes = rom.hashes()?;
    let size = rom.size.ok_or("size is missing or not a number")?;

    let file = match rom.header() {
        Some(header) => RomFile {
            size: size
                .checked_sub(header.len() as u64)
                .ok_or("header is larger than the ROM")?,
            rom_id: None,
            hashes: RomHashes::default(),
            status: rom.status(),
            non_canonical_header: Some(NonCanonicalHeader {
                bytes: header,
                hashes_with_header: hashes,
            }),
        },
        None => RomFile {
            size,
            rom_id: None,
            hashes,
            status: rom.status(),
            non_canonical_header: None,
        },
    };

    pending
        .variants
        .entry(data_file_name.to_string())
        .or_default()
        .insert(rom.path.replace('\\', "/"), file);

    Ok(())
}

fn program_info(game: &Game) -> ProgramInfo {
    let (title, tags) = split_name(&game.name);

    let mut regions = BTreeSet::new();
    let mut languages = BTreeSet::new();
    let mut flags = BTreeSet::new();
    let mut revision = None;

    for tag in &tags {
        let lowercase_tag = tag.to_lowercase();

        let parts: Vec<_> = lowercase_tag.split(',').map(str::trim).collect();

        let tag_regions: Vec<Region> = parts.iter().filter_map(|part| region(part)).collect();
        if !tag_regions.is_empty() && tag_regions.len() == parts.len() {
            regions.extend(tag_regions);

            for part in &parts {
                if let Some(language) = LANGUAGE_OVERRIDES.get(part) {
                    languages.insert(language.to_alpha3());
                }
            }

            continue;
        }

        let tag_languages: Vec<_> = parts
            .iter()
            .filter_map(|part| Iso639Alpha2::from_str(part).ok())
            .map(|language| language.to_alpha3())
            .collect();
        if !tag_languages.is_empty() && tag_languages.len() == parts.len() {
            languages = tag_languages.into_iter().collect();

            continue;
        }

        let words: Vec<_> = lowercase_tag.split_whitespace().collect();

        match words.as_slice() {
            ["rev", rest @ ..] if !rest.is_empty() => {
                revision = Some(tag.clone());
            }
            [version]
                if version.starts_with('v')
                    && version[1..].starts_with(|character: char| character.is_ascii_digit()) =>
            {
                revision = Some(tag.clone());
            }
            ["beta", ..] => {
                flags.insert(ProgramFlag::Beta);
            }
            ["proto", ..] | ["prototype", ..] => {
                flags.insert(ProgramFlag::Prototype);
            }
            ["sample", ..] => {
                flags.insert(ProgramFlag::Sample);
            }
            ["unl"] => {
                flags.insert(ProgramFlag::Unlicensed);
            }
            ["pirate"] => {
                flags.insert(ProgramFlag::Pirate);
            }
            ["homebrew"] | ["aftermarket"] => {
                flags.insert(ProgramFlag::Homebrew);
            }
            _ => {}
        }
    }

    ProgramInfo::V0 {
        title: Title {
            text: title,
            language: None,
        },
        alternative_titles: BTreeSet::new(),
        regions,
        languages,
        revision,
        kind: program_kind(&game.category),
        flags,
        video_standards: BTreeSet::new(),
        serials: BTreeSet::new(),
    }
}

fn split_name(name: &str) -> (String, Vec<String>) {
    let title_end = name.find(" (").unwrap_or(name.len());
    let title = name[..title_end].trim().to_string();

    (title, get_data_in_parentheses(&name[title_end..]))
}

fn program_kind(categories: &[String]) -> ProgramKind {
    categories
        .iter()
        .map(|category| match category.as_str() {
            "Games" | "Preproduction" => ProgramKind::Game,
            "Applications" => ProgramKind::Application,
            "Demos" => ProgramKind::Demo,
            "Educational" => ProgramKind::Educational,
            "Firmware" | "Boot" | "Console" | "MechaCon" => ProgramKind::Firmware,
            "Add-Ons" | "Updates" => ProgramKind::AddOn,
            "Audio" => ProgramKind::Audio,
            "Video" | "DVD" | "Multimedia" => ProgramKind::Video,
            _ => ProgramKind::Other,
        })
        .next()
        .unwrap_or_default()
}

fn region(name: &str) -> Option<Region> {
    match name {
        "world" => Some(Region::World),
        "europe" => Some(Region::Europe),
        "asia" => Some(Region::Asia),
        "latin america" => Some(Region::LatinAmerica),
        "scandinavia" => Some(Region::Scandinavia),
        _ => COUNTRY_ALIASES
            .get(name)
            .copied()
            .or_else(|| Iso3166Alpha2::from_english_name(name))
            .map(Region::Country),
    }
}

static COUNTRY_ALIASES: LazyLock<HashMap<&'static str, Iso3166Alpha2>> = LazyLock::new(|| {
    HashMap::from([
        ("usa", Iso3166Alpha2::US),
        ("uk", Iso3166Alpha2::GB),
        ("united kingdom", Iso3166Alpha2::GB),
        ("korea", Iso3166Alpha2::KR),
        ("hong kong", Iso3166Alpha2::HK),
        ("russia", Iso3166Alpha2::RU),
    ])
});

static LANGUAGE_OVERRIDES: LazyLock<HashMap<&'static str, Iso639Alpha2>> = LazyLock::new(|| {
    HashMap::from([
        ("usa", Iso639Alpha2::EN),
        ("japan", Iso639Alpha2::JA),
        ("china", Iso639Alpha2::ZH),
        ("korea", Iso639Alpha2::KO),
        ("australia", Iso639Alpha2::EN),
        ("canada", Iso639Alpha2::EN),
        ("united kingdom", Iso639Alpha2::EN),
        ("uk", Iso639Alpha2::EN),
        ("france", Iso639Alpha2::FR),
        ("brazil", Iso639Alpha2::PT),
        ("italy", Iso639Alpha2::IT),
        ("germany", Iso639Alpha2::DE),
        ("spain", Iso639Alpha2::ES),
        ("taiwan", Iso639Alpha2::ZH),
    ])
});

fn get_data_in_parentheses(input: &str) -> Vec<String> {
    let mut result = Vec::new();
    let mut stack = Vec::new();

    for (i, c) in input.char_indices() {
        match c {
            '(' => {
                stack.push(i);
            }
            ')' => {
                if let Some(start) = stack.pop() {
                    let substring = &input[start + 1..i];
                    result.push(substring.trim().to_string());
                }
            }
            _ => {}
        }
    }

    result
}
