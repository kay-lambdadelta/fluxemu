use std::{collections::BTreeSet, path::PathBuf, sync::LazyLock};

use audio::AudioSettings;
use fluxemu_input::{
    InputId,
    physical::{PhysicalInputDeviceId, hotkey::Hotkey},
};
use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use serde_inline_default::serde_inline_default;
use serde_with::serde_as;

use crate::{graphics::GraphicsSettings, input::PhysicalGamepadConfiguration};

/// Audio related config types
pub mod audio;
/// Graphics related config types
pub mod graphics;
/// Input configuration
pub mod input;

/// Directory that fluxemu will use as a "home" folder
pub static STORAGE_DIRECTORY: LazyLock<PathBuf> = LazyLock::new(|| {
    cfg_if::cfg_if! {
        if #[cfg(target_os = "espidf")] {
            PathBuf::from("/fluxemu")
        } else if #[cfg(target_os = "horizon")] {
            PathBuf::from("sdmc:/fluxemu")
        } else if #[cfg(target_os = "psp")] {
            PathBuf::from("ms0:/fluxemu")
        } else if #[cfg(any(target_family = "unix", target_os = "windows"))] {
            dirs::data_dir().unwrap().join("fluxemu")
        } else {
            compile_error!("Unsupported target");
        }
    }
});

/// Config location
pub static ENVIRONMENT_LOCATION: LazyLock<PathBuf> = LazyLock::new(|| {
    cfg_if::cfg_if! {
        if #[cfg(any(target_family = "unix", target_os = "windows"))] {
            dirs::config_dir().map(|directory| {
                directory.join("fluxemu")
            })
            .unwrap_or_else(|| STORAGE_DIRECTORY.clone()).join("config.ron")
        } else {
            STORAGE_DIRECTORY.join("config.ron")
        }
    }
});

#[serde_as]
#[serde_inline_default]
#[derive(Serialize, Deserialize, Debug)]
pub struct Environment {
    #[serde(default)]
    pub physical_input_configs: IndexMap<PhysicalInputDeviceId, PhysicalGamepadConfiguration>,
    #[serde(default)]
    pub hotkeys: IndexMap<BTreeSet<InputId>, Hotkey>,
    #[serde(default)]
    /// Graphics settings
    pub graphics_setting: GraphicsSettings,
    #[serde(default)]
    /// Audio settings
    pub audio_settings: AudioSettings,
    #[serde_inline_default(Environment::default().file_browser_home_directory)]
    /// The folder that the gui will show initially
    pub file_browser_home_directory: PathBuf,
    #[serde_inline_default(Environment::default().log_location)]
    /// Location where logs will be written
    pub log_location: PathBuf,
    #[serde_inline_default(Environment::default().database_location)]
    /// [redb] database location
    pub database_location: PathBuf,
    #[serde_inline_default(Environment::default().save_directory)]
    /// Directory where saves will be stored
    pub save_directory: PathBuf,
    #[serde_inline_default(Environment::default().snapshot_directory)]
    /// Directory where snapshots will be stored
    pub snapshot_directory: PathBuf,
    #[serde_inline_default(Environment::default().rom_store)]
    /// Directory where emulator will store imported roms
    pub rom_store: PathBuf,
}

impl Default for Environment {
    fn default() -> Self {
        Self {
            physical_input_configs: Default::default(),
            graphics_setting: Default::default(),
            audio_settings: Default::default(),
            file_browser_home_directory: STORAGE_DIRECTORY.clone(),
            log_location: STORAGE_DIRECTORY.join("log"),
            database_location: STORAGE_DIRECTORY.join("database.redb"),
            save_directory: STORAGE_DIRECTORY.join("saves"),
            snapshot_directory: STORAGE_DIRECTORY.join("snapshots"),
            rom_store: STORAGE_DIRECTORY.join("roms"),
            hotkeys: Default::default(),
        }
    }
}
