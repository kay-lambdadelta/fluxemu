use std::{
    collections::HashMap, fmt::Display, iter::once, path::Path, str::FromStr, sync::LazyLock,
};

use serde::{Deserialize, Serialize};
use serde_with::{DeserializeFromStr, SerializeDisplay};
use strum::{EnumDiscriminants, EnumIter, EnumString, IntoEnumIterator, IntoStaticStr};

mod extension;
mod guess;

/// Game systems organized by vendor
#[derive(
    Default,
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    EnumDiscriminants,
    SerializeDisplay,
    DeserializeFromStr,
)]
#[strum_discriminants(
    name(Vendor),
    derive(IntoStaticStr, EnumString),
    strum(serialize_all = "lowercase")
)]
pub enum SystemId {
    /// Nintendo systems
    Nintendo(NintendoSystem),
    /// Sega systems
    Sega(SegaSystem),
    /// Sony systems
    Sony(SonySystem),
    /// Atari systems
    Atari(AtariSystem),
    /// Systems that do not fit in the above vendors
    Other(OtherSystem),
    #[default]
    /// Unspecified system
    Unknown,
}

impl SystemId {
    /// Iterate over all possible game systems
    pub fn iter() -> impl Iterator<Item = Self> {
        NintendoSystem::iter()
            .map(SystemId::Nintendo)
            .chain(SegaSystem::iter().map(SystemId::Sega))
            .chain(SonySystem::iter().map(SystemId::Sony))
            .chain(AtariSystem::iter().map(SystemId::Atari))
            .chain(OtherSystem::iter().map(SystemId::Other))
            .chain(once(SystemId::Unknown))
    }

    /// Get a well known file extension for the files this system supports
    pub fn extension(self) -> Option<&'static str> {
        self.extensions().first().copied()
    }

    pub fn extensions(self) -> &'static [&'static str] {
        extension::get_extensions(self)
    }

    /// Attempt to guess the game system from several heuristics including file
    /// extension and file contents
    pub fn guess(path: Option<&Path>, data: Option<&[u8]>) -> Option<Self> {
        guess::guess(path, data)
    }

    /// Converts the name to a "Nointro" convention string
    pub fn to_nointro_string(&self) -> &'static str {
        match self {
            Self::Nintendo(NintendoSystem::GameBoy) => "Nintendo - Game Boy",
            Self::Nintendo(NintendoSystem::GameBoyColor) => "Nintendo - Game Boy Color",
            Self::Nintendo(NintendoSystem::GameBoyAdvance) => "Nintendo - Game Boy Advance",
            Self::Nintendo(NintendoSystem::GameCube) => "Nintendo - Nintendo GameCube",
            Self::Nintendo(NintendoSystem::Wii) => "Nintendo - Wii",
            Self::Nintendo(NintendoSystem::WiiU) => "Nintendo - Wii U",
            Self::Nintendo(NintendoSystem::SuperNintendoEntertainmentSystem) => {
                "Nintendo - Super Nintendo Entertainment System"
            }
            Self::Nintendo(NintendoSystem::NintendoEntertainmentSystem) => {
                "Nintendo - Nintendo Entertainment System"
            }
            Self::Nintendo(NintendoSystem::Nintendo64) => "Nintendo - Nintendo 64",
            Self::Nintendo(NintendoSystem::NintendoDS) => "Nintendo - Nintendo DS",
            Self::Nintendo(NintendoSystem::NintendoDSi) => "Nintendo - Nintendo DSi",
            Self::Nintendo(NintendoSystem::Nintendo3DS) => "Nintendo - Nintendo 3DS",
            Self::Nintendo(NintendoSystem::PokemonMini) => "Nintendo - Pokemon Mini",
            Self::Nintendo(NintendoSystem::VirtualBoy) => "Nintendo - Virtual Boy",
            Self::Sony(SonySystem::Playstation) => "Sony - PlayStation",
            Self::Sony(SonySystem::Playstation2) => "Sony - PlayStation 2",
            Self::Sony(SonySystem::Playstation3) => "Sony - PlayStation 3",
            Self::Sony(SonySystem::PlaystationPortable) => "Sony - PlayStation Portable",
            Self::Sony(SonySystem::PlaystationVita) => "Sony - PlayStation Vita",
            Self::Sega(SegaSystem::MasterSystem) => "Sega - Master System",
            Self::Sega(SegaSystem::GameGear) => "Sega - Game Gear",
            Self::Sega(SegaSystem::Genesis) => "Sega - Mega Drive - Genesis",
            Self::Sega(SegaSystem::SegaCD) => "Sega - Sega CD",
            Self::Sega(SegaSystem::Sega32X) => "Sega - 32X",
            Self::Other(OtherSystem::Chip8) => "Other - Chip8",
            Self::Atari(AtariSystem::_2600) => "Atari - 2600",
            Self::Atari(AtariSystem::_5200) => "Atari - 5200",
            Self::Atari(AtariSystem::_7800) => "Atari - 7800",
            Self::Atari(AtariSystem::Lynx) => "Atari - Atari Lynx",
            Self::Atari(AtariSystem::Jaguar) => "Atari - Jaguar",
            Self::Unknown => "Unknown",
        }
    }

    /// FIXME: This is written as stupidly as it could be
    pub fn from_nointro_str(s: &str) -> Result<Self, String> {
        let original_s = s;

        let s = strip_brackets_and_parens(
            &s.replace("Non-Redump -", "")
                .replace("Unofficial -", "")
                .replace("- BIOS Images", ""),
        )
        .trim()
        .to_lowercase()
        .replace(' ', "");

        static SYSTEMS_AS_NOINTRO_STRING: LazyLock<HashMap<String, SystemId>> =
            LazyLock::new(|| {
                SystemId::iter()
                    .map(|system| {
                        (
                            system.to_nointro_string().to_lowercase().replace(' ', ""),
                            system,
                        )
                    })
                    .collect()
            });

        if let Some(system) = SYSTEMS_AS_NOINTRO_STRING.get(&s) {
            return Ok(*system);
        }

        let company_names = [
            NintendoSystem::NOINTRO_NAME,
            SegaSystem::NOINTRO_NAME,
            SonySystem::NOINTRO_NAME,
            AtariSystem::NOINTRO_NAME,
            OtherSystem::NOINTRO_NAME,
        ];

        for company_name in company_names.map(str::to_lowercase) {
            if let Some(index) = s.rfind(&company_name) {
                let mut s_without_company = s.clone();
                s_without_company.replace_range(index..index + company_name.len(), "");

                if let Some(system) = SYSTEMS_AS_NOINTRO_STRING.get(&s_without_company) {
                    return Ok(*system);
                }
            }

            for (system_string, system) in SYSTEMS_AS_NOINTRO_STRING.iter() {
                if let Some(index) = system_string.rfind(&company_name) {
                    let mut system_string_without_company = system_string.clone();
                    system_string_without_company
                        .replace_range(index..index + company_name.len(), "");

                    if s == system_string_without_company {
                        return Ok(*system);
                    }
                }
            }
        }

        Err(format!("Unknown system: {original_s}"))
    }
}

impl SystemId {
    pub const VENDOR_SEPARATOR: char = '@';

    pub fn vendor(self) -> Vendor {
        self.into()
    }

    fn system_name(self) -> Option<&'static str> {
        match self {
            Self::Nintendo(system) => Some(system.into()),
            Self::Sega(system) => Some(system.into()),
            Self::Sony(system) => Some(system.into()),
            Self::Atari(system) => Some(system.into()),
            Self::Other(system) => Some(system.into()),
            Self::Unknown => None,
        }
    }
}

impl Display for SystemId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let vendor: &'static str = self.vendor().into();

        match self.system_name() {
            Some(system) => write!(f, "{system}{}{vendor}", Self::VENDOR_SEPARATOR),
            None => f.write_str(vendor),
        }
    }
}

impl FromStr for SystemId {
    type Err = &'static str;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let (system, vendor) = match s.split_once(Self::VENDOR_SEPARATOR) {
            Some((system, vendor)) => (Some(system), vendor),
            None => (None, s),
        };
        let vendor: Vendor = vendor.parse().map_err(|_| "Unknown vendor")?;
        let unknown_system = |_| "Unknown system";

        match (vendor, system) {
            (Vendor::Unknown, None) => Ok(Self::Unknown),
            (Vendor::Unknown, Some(_)) => Err("Unknown system has no vendor"),
            (_, None) => Err("Missing vendor separator"),
            (Vendor::Nintendo, Some(system)) => {
                system.parse().map(Self::Nintendo).map_err(unknown_system)
            }
            (Vendor::Sega, Some(system)) => system.parse().map(Self::Sega).map_err(unknown_system),
            (Vendor::Sony, Some(system)) => system.parse().map(Self::Sony).map_err(unknown_system),
            (Vendor::Atari, Some(system)) => {
                system.parse().map(Self::Atari).map_err(unknown_system)
            }
            (Vendor::Other, Some(system)) => {
                system.parse().map(Self::Other).map_err(unknown_system)
            }
        }
    }
}

#[allow(missing_docs)]
#[derive(
    Serialize,
    Deserialize,
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    EnumIter,
    IntoStaticStr,
    EnumString,
)]
/// All Nintendo systems
pub enum NintendoSystem {
    #[strum(serialize = "game-boy")]
    GameBoy,
    #[strum(serialize = "game-boy-color")]
    GameBoyColor,
    #[strum(serialize = "game-boy-advance")]
    GameBoyAdvance,
    #[strum(serialize = "gamecube")]
    GameCube,
    #[strum(serialize = "wii")]
    Wii,
    #[strum(serialize = "wii-u")]
    WiiU,
    #[strum(serialize = "nintendo-entertainment-system")]
    NintendoEntertainmentSystem,
    #[strum(serialize = "super-nintendo-entertainment-system")]
    SuperNintendoEntertainmentSystem,
    #[strum(serialize = "nintendo-64")]
    Nintendo64,
    #[strum(serialize = "nintendo-ds")]
    NintendoDS,
    #[strum(serialize = "nintendo-dsi")]
    NintendoDSi,
    #[strum(serialize = "nintendo-3ds")]
    Nintendo3DS,
    #[strum(serialize = "pokemon-mini")]
    PokemonMini,
    #[strum(serialize = "virtual-boy")]
    VirtualBoy,
}

#[allow(missing_docs)]
impl NintendoSystem {
    pub const NOINTRO_NAME: &str = "Nintendo";
}

#[allow(missing_docs)]
#[derive(
    Serialize,
    Deserialize,
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    EnumIter,
    IntoStaticStr,
    EnumString,
)]
/// All Sega systems
pub enum SegaSystem {
    #[strum(serialize = "master-system")]
    MasterSystem,
    #[strum(serialize = "game-gear")]
    GameGear,
    #[strum(serialize = "genesis")]
    Genesis,
    #[strum(serialize = "32x")]
    Sega32X,
    #[strum(serialize = "sega-cd")]
    SegaCD,
}

#[allow(missing_docs)]
impl SegaSystem {
    pub const NOINTRO_NAME: &str = "Sega";
}

#[allow(missing_docs)]
#[derive(
    Serialize,
    Deserialize,
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    EnumIter,
    IntoStaticStr,
    EnumString,
)]
/// All Sony systems
pub enum SonySystem {
    #[strum(serialize = "playstation")]
    Playstation,
    #[strum(serialize = "playstation-2")]
    Playstation2,
    #[strum(serialize = "playstation-3")]
    Playstation3,
    #[strum(serialize = "playstation-portable")]
    PlaystationPortable,
    #[strum(serialize = "playstation-vita")]
    PlaystationVita,
}

#[allow(missing_docs)]
impl SonySystem {
    pub const NOINTRO_NAME: &str = "Sony";
}

#[allow(missing_docs)]
#[derive(
    Serialize,
    Deserialize,
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    EnumIter,
    IntoStaticStr,
    EnumString,
)]
/// Some random assorted other systems
pub enum OtherSystem {
    #[strum(serialize = "chip-8")]
    Chip8,
}

#[allow(missing_docs)]
impl OtherSystem {
    pub const NOINTRO_NAME: &str = "Other";
}

#[allow(missing_docs)]
#[derive(
    Serialize,
    Deserialize,
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    EnumIter,
    IntoStaticStr,
    EnumString,
)]
/// All Atari systems
pub enum AtariSystem {
    #[serde(rename = "2600")]
    #[strum(serialize = "2600")]
    _2600,
    #[serde(rename = "5200")]
    #[strum(serialize = "5200")]
    _5200,
    #[serde(rename = "7800")]
    #[strum(serialize = "7800")]
    _7800,
    #[strum(serialize = "lynx")]
    Lynx,
    #[strum(serialize = "jaguar")]
    Jaguar,
}

#[allow(missing_docs)]
impl AtariSystem {
    pub const NOINTRO_NAME: &str = "Atari";
}

fn strip_brackets_and_parens(input: &str) -> String {
    let mut result = String::new();
    let mut skip_level = 0;

    for c in input.chars() {
        match c {
            '(' | '[' => skip_level += 1,
            ')' | ']' => {
                if skip_level > 0 {
                    skip_level -= 1;
                }
            }
            _ => {
                if skip_level == 0 {
                    result.push(c);
                }
            }
        }
    }

    result
}
