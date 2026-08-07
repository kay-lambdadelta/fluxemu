use std::{
    collections::HashMap, fmt::Display, iter::once, path::Path, str::FromStr, sync::LazyLock,
};

use serde::{Deserialize, Serialize};
use strum::{EnumIter, IntoEnumIterator};

mod extension;
mod guess;

/// Game systems organized by vendor
#[derive(
    Serialize, Deserialize, Default, Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash,
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
        extension::get_extension(self)
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

impl AsRef<str> for SystemId {
    fn as_ref(&self) -> &str {
        match self {
            // Nintendo
            SystemId::Nintendo(NintendoSystem::GameBoy) => "nintendo-game-boy",
            SystemId::Nintendo(NintendoSystem::GameBoyColor) => "nintendo-game-boy-color",
            SystemId::Nintendo(NintendoSystem::GameBoyAdvance) => "nintendo-game-boy-advance",
            SystemId::Nintendo(NintendoSystem::GameCube) => "nintendo-nintendo-gamecube",
            SystemId::Nintendo(NintendoSystem::Wii) => "nintendo-wii",
            SystemId::Nintendo(NintendoSystem::WiiU) => "nintendo-wii-u",
            SystemId::Nintendo(NintendoSystem::SuperNintendoEntertainmentSystem) => {
                "nintendo-super-nintendo-entertainment-system"
            }
            SystemId::Nintendo(NintendoSystem::NintendoEntertainmentSystem) => {
                "nintendo-nintendo-entertainment-system"
            }
            SystemId::Nintendo(NintendoSystem::Nintendo64) => "nintendo-nintendo-64",
            SystemId::Nintendo(NintendoSystem::NintendoDS) => "nintendo-nintendo-ds",
            SystemId::Nintendo(NintendoSystem::NintendoDSi) => "nintendo-nintendo-dsi",
            SystemId::Nintendo(NintendoSystem::Nintendo3DS) => "nintendo-nintendo-3ds",
            SystemId::Nintendo(NintendoSystem::PokemonMini) => "nintendo-pokemon-mini",
            SystemId::Nintendo(NintendoSystem::VirtualBoy) => "nintendo-virtual-boy",

            SystemId::Sony(SonySystem::Playstation) => "sony-playstation",
            SystemId::Sony(SonySystem::Playstation2) => "sony-playstation-2",
            SystemId::Sony(SonySystem::Playstation3) => "sony-playstation-3",
            SystemId::Sony(SonySystem::PlaystationPortable) => "sony-playstation-portable",
            SystemId::Sony(SonySystem::PlaystationVita) => "sony-playstation-vita",

            SystemId::Sega(SegaSystem::MasterSystem) => "sega-master-system",
            SystemId::Sega(SegaSystem::GameGear) => "sega-game-gear",
            SystemId::Sega(SegaSystem::Genesis) => "sega-genesis",
            SystemId::Sega(SegaSystem::SegaCD) => "sega-sega-cd",
            SystemId::Sega(SegaSystem::Sega32X) => "sega-32x",

            SystemId::Atari(AtariSystem::_2600) => "atari-2600",
            SystemId::Atari(AtariSystem::_5200) => "atari-5200",
            SystemId::Atari(AtariSystem::_7800) => "atari-7800",
            SystemId::Atari(AtariSystem::Lynx) => "atari-lynx",
            SystemId::Atari(AtariSystem::Jaguar) => "atari-jaguar",

            SystemId::Other(OtherSystem::Chip8) => "other-chip8",
            SystemId::Unknown => "unknown",
        }
    }
}

impl Display for SystemId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_ref())
    }
}

impl FromStr for SystemId {
    type Err = &'static str;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::iter()
            .find(|system| system.as_ref() == s)
            .ok_or("Could not parse")
    }
}

#[allow(missing_docs)]
#[derive(
    Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, EnumIter,
)]
/// All Nintendo systems
pub enum NintendoSystem {
    GameBoy,
    GameBoyColor,
    GameBoyAdvance,
    GameCube,
    Wii,
    WiiU,
    NintendoEntertainmentSystem,
    SuperNintendoEntertainmentSystem,
    Nintendo64,
    NintendoDS,
    NintendoDSi,
    Nintendo3DS,
    PokemonMini,
    VirtualBoy,
}

#[allow(missing_docs)]
impl NintendoSystem {
    pub const NOINTRO_NAME: &str = "Nintendo";
}

#[allow(missing_docs)]
#[derive(
    Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, EnumIter,
)]
/// All Sega systems
pub enum SegaSystem {
    MasterSystem,
    GameGear,
    Genesis,
    Sega32X,
    SegaCD,
}

#[allow(missing_docs)]
impl SegaSystem {
    pub const NOINTRO_NAME: &str = "Sega";
}

#[allow(missing_docs)]
#[derive(
    Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, EnumIter,
)]
/// All Sony systems
pub enum SonySystem {
    Playstation,
    Playstation2,
    Playstation3,
    PlaystationPortable,
    PlaystationVita,
}

#[allow(missing_docs)]
impl SonySystem {
    pub const NOINTRO_NAME: &str = "Sony";
}

#[allow(missing_docs)]
#[derive(
    Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, EnumIter,
)]
/// Some random assorted other systems
pub enum OtherSystem {
    Chip8,
}

#[allow(missing_docs)]
impl OtherSystem {
    pub const NOINTRO_NAME: &str = "Other";
}

#[allow(missing_docs)]
#[derive(
    Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, EnumIter,
)]
/// All Atari systems
pub enum AtariSystem {
    #[serde(rename = "2600")]
    _2600,
    #[serde(rename = "5200")]
    _5200,
    #[serde(rename = "7800")]
    _7800,
    Lynx,
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
