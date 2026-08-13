use std::{
    collections::HashMap,
    fs::File,
    io::LineWriter,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Instant,
};

use egui::{RawInput, Rect, ViewportId, ViewportInfo};
use fluxemu_environment::find_and_load_environment;
use fluxemu_frontend::{
    Frontend,
    graphics::{DrawTarget, GraphicsRuntime as _},
};
use fluxemu_program::ProgramManager;
use palette::named::BLACK;
use tracing::level_filters::LevelFilter;
use tracing_subscriber::{
    EnvFilter, Layer,
    fmt::format::FmtSpan,
    layer::{Filter, SubscriberExt},
    util::SubscriberInitExt,
};

use crate::{
    build_machine::get_software_factories,
    platform::Platform,
    runtime::{AudioRuntime, GraphicsRuntime},
};

mod build_machine;
mod platform;
mod runtime;

#[cfg(target_os = "nuttx")]
mod sys;

static ALREADY_RAN: AtomicBool = AtomicBool::new(false);

#[unsafe(no_mangle)]
unsafe extern "C" fn fluxemu_shell_nuttallite_main(_argc: i32, _argv: *const *const u8) -> i32 {
    if ALREADY_RAN.swap(true, Ordering::AcqRel) {
        println!("Cannot run again");

        return 1;
    }

    match main() {
        Ok(_) => 0,
        Err(err) => {
            println!("{}", err);

            1
        }
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let (environment_location, environment) = find_and_load_environment();

    let filter = Arc::new(
        EnvFilter::builder()
            .with_regex(true)
            .with_default_directive(LevelFilter::INFO.into())
            .from_env_lossy(),
    );

    let stderr_layer = tracing_subscriber::fmt::layer()
        .with_writer(|| LineWriter::new(std::io::stderr()))
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

    let program_manager = ProgramManager::new(None, environment.rom_store_directories.clone())?;

    let mut frontend = Frontend::<Platform>::new(
        environment,
        environment_location.into(),
        get_software_factories(),
        program_manager,
        AudioRuntime,
        None,
    );

    let mut graphics_runtime = GraphicsRuntime::default();

    let start_time = Instant::now();

    loop {
        frontend.maybe_reset_graphics_to_meet_machine_requirements(|_, sealed_machine_builder| {
            graphics_runtime.reconfigure(sealed_machine_builder.graphics_requirements());
            graphics_runtime.component_initialization_data()
        });

        if frontend.overlay_active() {
            let raw_input = RawInput {
                viewport_id: ViewportId::ROOT,
                viewports: HashMap::from_iter([(
                    ViewportId::ROOT,
                    ViewportInfo {
                        focused: Some(true),
                        fullscreen: Some(true),
                        native_pixels_per_point: Some(1.0),
                        ..Default::default()
                    },
                )]),
                screen_rect: Some(Rect {
                    min: [0.0, 0.0].into(),
                    max: [
                        graphics_runtime.texture.width() as f32,
                        graphics_runtime.texture.height() as f32,
                    ]
                    .into(),
                }),
                time: Some(start_time.elapsed().as_secs_f64()),
                focused: true,
                ..Default::default()
            };

            let full_output = frontend.run_menu(raw_input);

            graphics_runtime.present(
                BLACK,
                [DrawTarget::Egui {
                    context: frontend.egui_context(),
                    full_output,
                }],
            );
        } else if let Some(machine) = frontend.machine() {
            graphics_runtime.present(BLACK, [DrawTarget::Machine { machine }]);
        }
    }
}
