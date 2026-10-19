use std::{
    ffi::{CStr, c_char},
    fs::File,
    io::LineWriter,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};

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

mod build_machine;
mod frontend;
mod platform;
mod runtime;

#[cfg(all(not(feature = "egui"), not(feature = "ratatui")))]
compile_error!(
    "No frontend enabled, please enable one of the supported frontends (egui and/or ratatui)"
);

#[cfg(target_os = "nuttx")]
mod storage;
#[cfg(target_os = "nuttx")]
mod sys;

static ALREADY_RAN: AtomicBool = AtomicBool::new(false);

#[unsafe(no_mangle)]
unsafe extern "C" fn fluxemu_shell_nuttallite_main(argc: i32, argv: *const *const u8) -> i32 {
    if ALREADY_RAN.swap(true, Ordering::AcqRel) {
        println!("Cannot run again");

        return 1;
    }

    // FIXME: Our intended way to run nuttallite skips nsh, which means this code would be dead in our provided builds
    // TODO: Make this use clap somehow

    let argument_count = if argv.is_null() {
        0
    } else {
        usize::try_from(argc).unwrap_or(0)
    };

    let arguments = (1..argument_count)
        .map(|index| unsafe { *argv.add(index) })
        .filter(|argument| !argument.is_null())
        .map(|argument| {
            unsafe { CStr::from_ptr(argument.cast::<c_char>()) }
                .to_string_lossy()
                .into_owned()
        })
        .collect::<Vec<_>>();

    match main(&arguments) {
        Ok(_) => 0,
        Err(err) => {
            println!("{}", err);

            1
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
enum FrontendSelection {
    #[cfg(feature = "egui")]
    #[default]
    Egui,
    #[cfg(feature = "ratatui")]
    #[cfg_attr(not(feature = "egui"), default)]
    Ratatui,
}

impl FrontendSelection {
    fn from_arguments(arguments: &[String]) -> Self {
        arguments
            .iter()
            .find_map(|argument| match argument.as_str() {
                #[cfg(feature = "egui")]
                "egui" => Some(Self::Egui),
                #[cfg(feature = "ratatui")]
                "ratatui" => Some(Self::Ratatui),
                _ => None,
            })
            .unwrap_or_default()
    }
}

fn main(arguments: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    #[cfg(target_os = "nuttx")]
    {
        storage::mount_storage_partition()?;
    }

    let environment = load_environment();
    let frontend_selection = FrontendSelection::from_arguments(arguments);

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

    #[cfg(feature = "ratatui")]
    let stderr_layer = (frontend_selection != FrontendSelection::Ratatui).then_some(stderr_layer);

    let (notification_layer, notifications) = notification_channel();

    let subscriber_builder = tracing_subscriber::registry()
        .with(stderr_layer.with_filter(filter.clone() as Arc<_>))
        .with(notification_layer.with_filter(filter.clone() as Arc<_>));

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

    match frontend_selection {
        #[cfg(feature = "egui")]
        FrontendSelection::Egui => frontend::egui::run(environment, program_manager, notifications),
        #[cfg(feature = "ratatui")]
        FrontendSelection::Ratatui => {
            frontend::ratatui::run(environment, program_manager, notifications)
        }
    }
}
