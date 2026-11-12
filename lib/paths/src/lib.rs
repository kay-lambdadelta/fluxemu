use std::{
    collections::BTreeMap, fs::create_dir_all, num::Wrapping, ops::Deref, path::PathBuf,
    sync::LazyLock, time::Duration,
};

use confique::Config;
use fluxemu_environment::{Environment, audio::AudioSettings, graphics::GraphicsSettings};
use fluxemu_program::RomStoreSpecification;
use indexmap::IndexMap;
use ron::{Options, extensions::Extensions, ser::PrettyConfig};

pub static STORAGE_DIRECTORY: LazyLock<PathBuf> = LazyLock::new(|| {
    cfg_select! {
        target_os = "nuttx" => PathBuf::from("/data/fluxemu"),
        all(any(target_family = "unix", target_os = "windows"),) => dirs::data_dir()
            .expect("Could not lookup data directory")
            .join("fluxemu"),
    }
});

pub static CACHE_DIRECTORY: LazyLock<PathBuf> = LazyLock::new(|| {
    cfg_select! {
        target_os = "nuttx" => STORAGE_DIRECTORY.clone(),
        all(any(target_family = "unix", target_os = "windows"),) => dirs::cache_dir()
            .map(|path| path.join("fluxemu"))
            .unwrap_or(STORAGE_DIRECTORY.clone()),
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
    let _ = create_dir_all(STORAGE_DIRECTORY.deref());
    let _ = create_dir_all(ENVIRONMENT_LOCATION.deref().parent().unwrap());

    let default_environment_string = ron::to_string(&Environment {
        gamepads: BTreeMap::default(),
        graphics: GraphicsSettings::default(),
        audio: AudioSettings::default(),
        file_browser_home_directory: std::env::home_dir().unwrap_or(STORAGE_DIRECTORY.clone()),
        log_location: STORAGE_DIRECTORY.join("log"),
        database_location: STORAGE_DIRECTORY.join("database.redb"),
        digest_cache_location: CACHE_DIRECTORY.join("digests.redb"),
        save_directory: STORAGE_DIRECTORY.join("saves"),
        snapshot_directory: STORAGE_DIRECTORY.join("snapshots"),
        rom_stores: IndexMap::from_iter([(
            "default".to_string(),
            RomStoreSpecification::filesystem(STORAGE_DIRECTORY.join("roms")),
        )]),
        active_snapshot_slot: Wrapping(0),
        check_archive_hashes: true,
        save_flush_interval: Duration::from_secs(30),
        snapshot_screenshots: true,
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

    let environment = config_builder
        .preloaded(
            Options::default()
                .with_default_extension(Extensions::IMPLICIT_SOME)
                .from_str(&default_environment_string)
                .unwrap(),
        )
        .load()
        .unwrap();

    // Try to create all relevant directories
    let _ = create_dir_all(&environment.file_browser_home_directory);
    let _ = create_dir_all(environment.log_location.parent().unwrap());
    let _ = create_dir_all(environment.database_location.parent().unwrap());
    let _ = create_dir_all(environment.digest_cache_location.parent().unwrap());
    let _ = create_dir_all(&environment.save_directory);
    let _ = create_dir_all(&environment.snapshot_directory);

    for directory in environment
        .rom_stores
        .values()
        .filter_map(RomStoreSpecification::filesystem_root)
    {
        let _ = create_dir_all(directory);
    }

    environment
}

pub fn save_environment(
    environment: &Environment,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let environment = Options::default().to_string_pretty(environment, PrettyConfig::new())?;
    std::fs::write(ENVIRONMENT_LOCATION.deref(), environment)?;

    Ok(())
}
