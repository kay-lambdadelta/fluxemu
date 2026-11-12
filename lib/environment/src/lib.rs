use std::{collections::BTreeMap, num::Wrapping, path::PathBuf, time::Duration};

use audio::AudioSettings;
use confique::Config;
use fluxemu_input::physical::PhysicalInputDeviceId;
use fluxemu_program::RomStoreSpecification;
use indexmap::IndexMap;
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
    #[config(env = "FLUXEMU_DIGEST_CACHE_LOCATION")]
    pub digest_cache_location: PathBuf,
    #[config(env = "FLUXEMU_SAVE_DIRECTORY")]
    pub save_directory: PathBuf,
    #[config(env = "FLUXEMU_SNAPSHOT_DIRECTORY")]
    pub snapshot_directory: PathBuf,
    pub rom_stores: IndexMap<String, RomStoreSpecification>,
    pub active_snapshot_slot: Wrapping<u8>,
    pub check_archive_hashes: bool,
    pub save_flush_interval: Duration,
    pub snapshot_screenshots: bool,
}
