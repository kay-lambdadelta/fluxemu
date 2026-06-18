//! A multisystem hardware emulator

// Make sure this does not spawn with the console on windows
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

#[cfg(all(not(feature = "drm"), not(feature = "windowing")))]
compile_error!(
    "No display backend enabled, please enable one of the supported backends (drm and/or \
     windowing)"
);

#[cfg(all(feature = "drm", not(target_os = "linux")))]
compile_error!("The DRM/KMS backend is only compatible with Linux");

use std::{
    fs::{File, create_dir_all},
    ops::Deref,
    sync::Arc,
};

use clap::Parser;
use cli::{Cli, CliAction};
use fluxemu_environment::{ENVIRONMENT_LOCATION, Environment, STORAGE_DIRECTORY};
use fluxemu_input::physical::hotkey::default_hotkeys;
use fluxemu_program::ProgramManager;
use redb::Database;
use ron::ser::PrettyConfig;
use tracing::level_filters::LevelFilter;
use tracing_subscriber::{
    EnvFilter, Layer,
    fmt::format::FmtSpan,
    layer::{Filter, SubscriberExt},
    util::SubscriberInitExt,
};

use crate::{display::software::SoftwareGraphicsRuntime, event_loop::DisplayBackend};

mod audio;
mod build_machine;
mod cli;
mod display;
mod event_loop;
mod input;
mod platform;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let _ = create_dir_all(STORAGE_DIRECTORY.deref());
    let _ = create_dir_all(ENVIRONMENT_LOCATION.parent().unwrap());

    let filter = Arc::new(
        EnvFilter::builder()
            .with_regex(true)
            .with_default_directive(LevelFilter::INFO.into())
            .from_env_lossy()
            .add_directive("cosmic_text=info".parse().unwrap())
            .add_directive("winit=info".parse().unwrap()),
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

    let stderr_layer = tracing_subscriber::fmt::layer()
        .with_writer(std::io::stderr)
        .with_ansi(true)
        .with_span_events(FmtSpan::CLOSE)
        .with_thread_names(true)
        .with_thread_ids(false);

    let subscriber_builder = tracing_subscriber::registry()
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
    let program_manager = ProgramManager::new(database, [environment.rom_store.clone()])?;

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
        // Software backend is always available
        fluxemu_environment::graphics::GraphicsApi::Software => match cli.display_backend {
            #[cfg(feature = "windowing")]
            DisplayBackend::Windowing => {
                use crate::event_loop::windowing::WindowingEventLoop;

                WindowingEventLoop::<SoftwareGraphicsRuntime<_>>::run(
                    environment.clone(),
                    program_manager.clone(),
                    build_machine::get_software_factories(),
                    initial_program.clone(),
                )?;
            }
            #[cfg(feature = "drm")]
            DisplayBackend::Drm => {
                use crate::event_loop::drm::DrmEventLoop;

                DrmEventLoop::<SoftwareGraphicsRuntime<_>>::run(
                    environment.clone(),
                    program_manager.clone(),
                    build_machine::get_software_factories(),
                    initial_program.clone(),
                )?;
            }
        },
        #[cfg(feature = "webgpu")]
        fluxemu_environment::graphics::GraphicsApi::Webgpu => {
            use crate::display::webgpu::WebgpuGraphicsRuntime;

            match cli.display_backend {
                #[cfg(feature = "windowing")]
                DisplayBackend::Windowing => {
                    use crate::event_loop::windowing::WindowingEventLoop;

                    WindowingEventLoop::<WebgpuGraphicsRuntime<_>>::run(
                        environment.clone(),
                        program_manager.clone(),
                        build_machine::get_webgpu_factories(),
                        initial_program.clone(),
                    )?;
                }
                #[cfg(feature = "drm")]
                DisplayBackend::Drm => {
                    use crate::event_loop::drm::DrmEventLoop;

                    DrmEventLoop::<WebgpuGraphicsRuntime<_>>::run(
                        environment.clone(),
                        program_manager.clone(),
                        build_machine::get_webgpu_factories(),
                        initial_program.clone(),
                    )?;
                }
            }
        }
        _ => todo!(),
    }

    Ok(())
}
