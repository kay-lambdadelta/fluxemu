//! A multisystem hardware emulator

// Make sure this does not spawn with the console on windows
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

#[cfg(all(
    not(feature = "drm"),
    not(feature = "windowing"),
    not(feature = "ratatui")
))]
compile_error!(
    "No display backend enabled, please enable one of the supported backends (drm and/or \
     windowing)"
);

#[cfg(all(not(feature = "egui"), not(feature = "ratatui")))]
compile_error!(
    "No frontend enabled, please enable one of the supported frontends (egui and/or ratatui)"
);

#[cfg(all(feature = "drm", not(target_os = "linux")))]
compile_error!("The DRM/KMS backend is only compatible with Linux");

#[cfg(feature = "egui")]
mod font;

use std::{fs::File, sync::Arc};

use clap::Parser;
use cli::{Cli, CliAction};
use fluxemu_environment::load_environment;
use fluxemu_frontend::notification::notification_channel;
use fluxemu_program::ProgramManager;
use redb::Database;
use tracing::level_filters::LevelFilter;
use tracing_subscriber::{
    EnvFilter, Layer,
    fmt::format::FmtSpan,
    layer::{Filter, SubscriberExt},
    util::SubscriberInitExt,
};

use crate::event_loop::DisplayBackend;

mod audio;
mod build_machine;
mod cli;
#[cfg(feature = "egui")]
mod display;
mod event_loop;
mod gamepad;
mod platform;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let environment = load_environment();
    let cli = Cli::parse();

    let filter = Arc::new(
        EnvFilter::builder()
            .with_regex(true)
            .with_default_directive(LevelFilter::INFO.into())
            .from_env_lossy()
            .add_directive("cosmic_text=info".parse().unwrap())
            .add_directive("winit=info".parse().unwrap())
            .add_directive("egui_wgpu::renderer=off".parse().unwrap()),
    );

    let stderr_layer = tracing_subscriber::fmt::layer()
        .with_writer(std::io::stderr)
        .with_ansi(true)
        .with_span_events(FmtSpan::CLOSE)
        .with_thread_names(true)
        .with_thread_ids(false);

    let (notification_layer, notifications) = notification_channel();

    // Do not break ratatui if its active
    #[cfg(feature = "ratatui")]
    let stderr_layer = (cli.display_backend != DisplayBackend::Ratatui).then_some(stderr_layer);

    let subscriber_builder = tracing_subscriber::registry()
        .with(stderr_layer.with_filter(filter.clone() as Arc<dyn Filter<_> + Send + Sync>))
        .with(notification_layer.with_filter(filter.clone() as Arc<dyn Filter<_> + Send + Sync>));

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
    let program_manager = ProgramManager::new(database, environment.rom_store_directories.clone())?;

    let mut initial_program = None;

    if let Some(action) = cli.action {
        match action {
            CliAction::Run { roms } => {
                let mut rom_ids = Vec::default();

                for rom in &roms {
                    rom_ids.push(program_manager.register_external_rom(rom)?);
                }

                initial_program = Some(rom_ids);
            }
        };
    }

    #[cfg(feature = "ratatui")]
    if cli.display_backend == DisplayBackend::Ratatui {
        use fluxemu_frontend_ratatui::TerminalGraphicsRuntime;

        use crate::{audio::CpalAudioRuntime, gamepad::GamepadContext, platform::DesktopPlatform};

        let (mut external_input_sender, external_input_receiver) =
            fluxemu_frontend_ratatui::external_input_channel();

        match GamepadContext::new(&mut external_input_sender) {
            Ok(mut context) => {
                std::thread::Builder::new()
                    .name("Gamepad Poll Thread".to_string())
                    .spawn(move || {
                        loop {
                            if let Some(callback) = context.poll_gamepad_events(None) {
                                callback(&mut external_input_sender);
                            }
                        }
                    })?;
            }
            Err(err) => {
                tracing::error!(
                    "Gamepad context could not be created: {}, you will not have gamepad support",
                    err
                );
            }
        }

        let audio_runtime =
            CpalAudioRuntime::new().map_err(|_| "Could not initialize audio runtime")?;

        fluxemu_frontend_ratatui::run::<DesktopPlatform<TerminalGraphicsRuntime, false>>(
            environment,
            program_manager,
            // TODO: Support webgpu for the ratatui backend
            build_machine::get_software_factories(),
            audio_runtime,
            initial_program,
            notifications,
            external_input_receiver,
        )?;

        return Ok(());
    }

    #[cfg(feature = "egui")]
    match environment.graphics.api {
        // Software backend is always available
        fluxemu_environment::graphics::GraphicsApi::Software => match cli.display_backend {
            #[cfg(feature = "windowing")]
            DisplayBackend::Windowing => {
                crate::event_loop::windowing::run::<
                    crate::display::software::SoftwareGraphicsRuntime<_>,
                >(
                    environment.clone(),
                    program_manager.clone(),
                    build_machine::get_software_factories(),
                    initial_program.clone(),
                    notifications,
                )?;
            }
            #[cfg(feature = "drm")]
            DisplayBackend::Drm => {
                crate::event_loop::drm::run::<crate::display::software::SoftwareGraphicsRuntime<_>>(
                    environment.clone(),
                    program_manager.clone(),
                    build_machine::get_software_factories(),
                    initial_program.clone(),
                    notifications,
                )?;
            }
            #[cfg(feature = "ratatui")]
            DisplayBackend::Ratatui => {
                unreachable!()
            }
        },
        #[cfg(feature = "webgpu")]
        fluxemu_environment::graphics::GraphicsApi::Webgpu => {
            use crate::display::webgpu::WebgpuGraphicsRuntime;

            match cli.display_backend {
                #[cfg(feature = "windowing")]
                DisplayBackend::Windowing => {
                    crate::event_loop::windowing::run::<WebgpuGraphicsRuntime<_>>(
                        environment.clone(),
                        program_manager.clone(),
                        build_machine::get_webgpu_factories(),
                        initial_program.clone(),
                        notifications,
                    )?;
                }
                #[cfg(feature = "drm")]
                DisplayBackend::Drm => {
                    crate::event_loop::drm::run::<WebgpuGraphicsRuntime<_>>(
                        environment.clone(),
                        program_manager.clone(),
                        build_machine::get_webgpu_factories(),
                        initial_program.clone(),
                        notifications,
                    )?;
                }
                #[cfg(feature = "ratatui")]
                DisplayBackend::Ratatui => {
                    unreachable!()
                }
            }
        }
        _ => todo!(),
    }

    Ok(())
}
