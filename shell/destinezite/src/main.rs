//! A multisystem hardware emulator

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::{
    fs::{File, create_dir_all},
    ops::Deref,
    sync::Arc,
};

use clap::Parser;
use cli::{Cli, CliAction};
use egui_tracing::EventCollector;
use fluxemu_environment::{ENVIRONMENT_LOCATION, Environment, STORAGE_DIRECTORY};
use fluxemu_input::physical::hotkey::default_hotkeys;
use fluxemu_program::ProgramManager;
use redb::Database;
use ron::ser::PrettyConfig;
use tracing::{Level, level_filters::LevelFilter};
use tracing_subscriber::{
    EnvFilter, Layer,
    layer::{Filter, SubscriberExt},
    util::SubscriberInitExt,
};

use crate::{backend::software::SoftwareGraphicsRuntime, windowing::DesktopEventLoop};

mod audio;
mod backend;
mod build_machine;
mod cli;
mod input;
mod platform;
mod windowing;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let _ = create_dir_all(STORAGE_DIRECTORY.deref());
    let _ = create_dir_all(ENVIRONMENT_LOCATION.parent().unwrap());

    let filter = Arc::new(
        EnvFilter::builder()
            .with_regex(true)
            .with_default_directive(LevelFilter::INFO.into())
            .from_env_lossy()
            // Creates a bunch of spam presumably relating to benchmarking itself
            .add_directive("cosmic_text=info".parse().unwrap()),
    );

    let mut environment = if let Ok(environment_string) =
        std::fs::read_to_string(ENVIRONMENT_LOCATION.deref())
        && let Ok(environment) = ron::from_str(&environment_string)
    {
        environment
    } else {
        Environment::default()
    };

    if environment.hotkeys.is_empty() {
        environment.hotkeys = default_hotkeys().collect();
    }

    if !ENVIRONMENT_LOCATION.is_file() {
        let environment_string = ron::ser::to_string_pretty(&environment, PrettyConfig::default())?;

        if let Err(error) = std::fs::write(ENVIRONMENT_LOCATION.deref(), environment_string) {
            tracing::error!("Failed to write environment file: {}", error);
        }
    }

    let tracing_event_collector = EventCollector::new().with_max_level(
        filter
            .max_level_hint()
            .and_then(|level_filter| level_filter.into_level())
            .unwrap_or(Level::INFO),
    );
    let stderr_layer = tracing_subscriber::fmt::layer()
        .with_writer(std::io::stderr)
        .with_ansi(true)
        .with_thread_names(true);

    let subscriber_builder = tracing_subscriber::registry()
        .with(
            tracing_event_collector
                .clone()
                .with_filter(filter.clone() as Arc<dyn Filter<_> + Send + Sync>),
        )
        .with(stderr_layer.with_filter(filter.clone() as Arc<dyn Filter<_> + Send + Sync>));

    if let Ok(file) = File::create(&environment.log_location) {
        let file_layer = tracing_subscriber::fmt::layer()
            .with_writer(file)
            .with_ansi(false);

        subscriber_builder
            .with(file_layer.with_filter(filter.clone() as Arc<dyn Filter<_> + Send + Sync>))
            .init();
    } else {
        subscriber_builder.init();

        tracing::error!(
            "Could not enable mirroring log to local file at {}",
            environment.log_location.display()
        );
    }

    tracing::info!("FluxEMU v{}", env!("CARGO_PKG_VERSION"));

    let database = Database::create(&environment.database_location)?;
    let program_manager = ProgramManager::new(database)?;

    let cli = Cli::parse();

    let mut initial_program = None;

    if let Some(action) = cli.action {
        match action {
            CliAction::Run { roms } => {
                let mut rom_ids = Vec::default();

                for rom in &roms {
                    rom_ids.push(program_manager.register_external(rom)?);
                }

                initial_program = Some(rom_ids);
            }
        };
    }

    match environment.graphics_setting.api {
        fluxemu_environment::graphics::GraphicsApi::Software => {
            DesktopEventLoop::<SoftwareGraphicsRuntime>::run(
                environment,
                tracing_event_collector,
                program_manager.clone(),
                build_machine::get_software_factories(),
                initial_program,
            )?;
        }
        #[cfg(feature = "webgpu")]
        fluxemu_environment::graphics::GraphicsApi::Webgpu => {
            use crate::backend::webgpu::WebgpuGraphicsRuntime;

            DesktopEventLoop::<WebgpuGraphicsRuntime>::run(
                environment,
                tracing_event_collector,
                program_manager.clone(),
                build_machine::get_webgpu_factories(),
                initial_program,
            )?;
        }
        _ => todo!(),
    }

    Ok(())
}
