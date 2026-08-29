use std::{collections::BTreeMap, num::Wrapping, ops::Deref, path::PathBuf, sync::LazyLock};

use audio::AudioSettings;
use confique::Config;
use fluxemu_input::physical::PhysicalInputDeviceId;
use ron::{Options, extensions::Extensions};
use serde::{Deserialize, Serialize};

use crate::{graphics::GraphicsSettings, input::PhysicalGamepadConfiguration};

/// Audio related config types
pub mod audio;
/// Graphics related config types
pub mod graphics;
/// Input configuration
pub mod input;

#[derive(Config, Serialize, Deserialize, Debug, Clone)]
pub struct Environment {
    pub gamepads: BTreeMap<PhysicalInputDeviceId, PhysicalGamepadConfiguration>,
    pub graphics: GraphicsSettings,
    pub audio: AudioSettings,
    pub file_browser_home_directory: PathBuf,
    #[config(env = "FLUXEMU_LOG_LOCATION")]
    pub log_location: PathBuf,
    #[config(env = "FLUXEMU_DATABASE_LOCATION")]
    pub database_location: PathBuf,
    #[config(env = "FLUXEMU_SAVE_DIRECTORY")]
    pub save_directory: PathBuf,
    #[config(env = "FLUXEMU_SNAPSHOT_DIRECTORY")]
    pub snapshot_directory: PathBuf,
    #[config(env = "FLUXEMU_ROM_STORE_DIRECTORIES")]
    pub rom_store_directories: Vec<PathBuf>,
    pub active_snapshot_slot: Wrapping<u8>,
}

pub static STORAGE_DIRECTORY: LazyLock<PathBuf> = LazyLock::new(|| {
    cfg_select! {
        target_os = "nuttx" => PathBuf::from("/data/fluxemu"),
        all(any(target_family = "unix", target_os = "windows"),) => dirs::data_dir()
            .expect("Could not lookup data directory")
            .join("fluxemu"),
    }
});

pub static ENVIRONMENT_LOCATION: LazyLock<PathBuf> = LazyLock::new(|| {
    cfg_select! {
        target_os = "nuttx" => STORAGE_DIRECTORY.join("environment.ron"),
        all(any(target_family = "unix", target_os = "windows"),) => dirs::config_dir()
            .map(|path| path.join("fluxemu"))
            .unwrap_or(STORAGE_DIRECTORY.clone())
            .join("environment.ron"),
    }
});

pub fn load_environment() -> Environment {
    let _ = std::fs::create_dir_all(STORAGE_DIRECTORY.deref());
    let _ = std::fs::create_dir_all(ENVIRONMENT_LOCATION.deref().parent().unwrap());

    let default_environment_string = ron::to_string(&Environment {
        gamepads: BTreeMap::default(),
        graphics: GraphicsSettings::default(),
        audio: AudioSettings::default(),
        file_browser_home_directory: std::env::home_dir().unwrap_or(STORAGE_DIRECTORY.clone()),
        log_location: STORAGE_DIRECTORY.join("log"),
        database_location: STORAGE_DIRECTORY.join("database.redb"),
        save_directory: STORAGE_DIRECTORY.join("saves"),
        snapshot_directory: STORAGE_DIRECTORY.join("snapshot"),
        rom_store_directories: vec![STORAGE_DIRECTORY.join("roms")],
        active_snapshot_slot: Wrapping(0),
    })
    .unwrap();

    let config_builder = Environment::builder().env();

    let config_builder = if let Ok(loaded_environment) =
        std::fs::read_to_string(ENVIRONMENT_LOCATION.deref())
        && let Ok(loaded_environment) = Options::default()
            .with_default_extension(Extensions::IMPLICIT_SOME)
            .from_str(&loaded_environment)
    {
        config_builder.preloaded(loaded_environment)
    } else {
        config_builder
    };

    config_builder
        .preloaded(
            Options::default()
                .with_default_extension(Extensions::IMPLICIT_SOME)
                .from_str(&default_environment_string)
                .unwrap(),
        )
        .load()
        .unwrap()
}
