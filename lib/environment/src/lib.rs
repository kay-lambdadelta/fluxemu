use std::{collections::BTreeMap, num::Wrapping, path::PathBuf};

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

pub fn find_and_load_environment() -> (PathBuf, Environment) {
    let storage_directory = dirs::data_dir()
        .expect("Could not lookup data directory")
        .join("fluxemu");

    let environment_location = dirs::config_dir()
        .map(|path| path.join("fluxemu"))
        .unwrap_or(storage_directory.clone())
        .join("environment.ron");

    let _ = std::fs::create_dir_all(&storage_directory);
    let _ = std::fs::create_dir_all(environment_location.parent().unwrap());

    let default_environment_string = ron::to_string(&Environment {
        gamepads: BTreeMap::default(),
        graphics: GraphicsSettings::default(),
        audio: AudioSettings::default(),
        file_browser_home_directory: std::env::home_dir().unwrap_or(storage_directory.clone()),
        log_location: storage_directory.join("log"),
        database_location: storage_directory.join("database.redb"),
        save_directory: storage_directory.join("saves"),
        snapshot_directory: storage_directory.join("snapshot"),
        rom_store_directories: vec![storage_directory.join("roms")],
        active_snapshot_slot: Wrapping(0),
    })
    .unwrap();

    let config_builder = Environment::builder()
        .preloaded(
            Options::default()
                .with_default_extension(Extensions::IMPLICIT_SOME)
                .from_str(&default_environment_string)
                .unwrap(),
        )
        .env();

    let environment = config_builder.load().unwrap();

    (environment_location, environment)
}
