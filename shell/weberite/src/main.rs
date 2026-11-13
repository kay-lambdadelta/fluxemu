use std::{
    cell::RefCell,
    path::PathBuf,
    rc::Rc,
    sync::mpsc::{Receiver, channel},
};

use fluxemu_database::ProgramDatabase;
use fluxemu_frontend::{FrontendCore, FrontendEvent, graphics::PassthroughGraphicsRuntime};
use fluxemu_program::{DigestCache, ProgramManager};
use redb::Database;
use tokio::{runtime::Runtime, task::LocalSet};
use tracing_subscriber::{filter::LevelFilter, layer::SubscriberExt, util::SubscriberInitExt};

use crate::{
    build_machine::get_software_factories, debug_overlay::DebugOverlay, display::CanvasDisplay,
    platform::Platform, runtime::AudioRuntime,
};

mod build_machine;
mod debug_overlay;
mod display;
mod frame_loop;
mod input;
mod platform;
mod rom_picker;
mod runtime;
mod storage;

// FIXME: When possible, all the ad-hoc handwritten canvas and keyboard code should be replaced with winit and related crates
//
// This will probably be possible in winit 0.31

struct Application {
    runtime: Runtime,
    local_set: LocalSet,
    core: FrontendCore<Platform>,
    display: Option<CanvasDisplay>,
    debug_overlay: Option<DebugOverlay>,
    picked_roms: Receiver<PathBuf>,
}

impl Application {
    fn frame(&mut self) {
        self.runtime
            .block_on(self.local_set.run_until(tokio::task::yield_now()));

        let _local_set_guard = self.local_set.enter();

        let picked_roms: Vec<_> = self.picked_roms.try_iter().collect();
        if !picked_roms.is_empty() {
            self.core.load_roms(picked_roms);
        }

        self.core.poll(&mut PassthroughGraphicsRuntime);

        if self.core.has_pending_machine() {
            self.core.start_pending_machine(|_| ());
        }

        for event in self.core.drain_events() {
            match event {
                FrontendEvent::DebugOverlayToggleRequested => {
                    if let Some(debug_overlay) = &mut self.debug_overlay {
                        debug_overlay.toggle();
                    }
                }
                FrontendEvent::MenuToggleRequested => {}
            }
        }

        if let Some(debug_overlay) = &mut self.debug_overlay {
            debug_overlay.update(self.core.controller());
        }

        if let Some(display) = &mut self.display
            && let Some((frame, _)) = self.core.capture_framebuffer()
            && let Err(err) = display.present(frame)
        {
            tracing::error!("Could not present frame: {}", err);
        }
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::registry()
        .with(LevelFilter::INFO)
        .with(
            tracing_subscriber::fmt::layer()
                .with_ansi(false)
                .with_writer(std::io::stderr),
        )
        .init();

    tracing::info!("FluxEMU v{}", env!("CARGO_PKG_VERSION"));

    if let Err(err) = storage::mount_storage() {
        tracing::error!("{}, storage will not persist", err);
    }

    opendal::init_default_registry();

    let environment = fluxemu_paths::load_environment();

    let runtime = Runtime::new()?;
    let program_database = ProgramDatabase::new(Database::create(&environment.database_location)?)?;
    let program_manager = ProgramManager::with_digest_cache(
        environment.rom_stores.clone(),
        DigestCache::try_open(&environment.digest_cache_location),
    );

    let local_set = LocalSet::new();

    let core = {
        let _local_set_guard = local_set.enter();

        FrontendCore::<Platform>::new(
            runtime.handle().clone(),
            environment,
            get_software_factories(),
            program_manager,
            program_database,
            AudioRuntime,
            None,
        )
    };

    let (picked_roms_sender, picked_roms) = channel();

    let display = CanvasDisplay::new()
        .inspect_err(|err| tracing::warn!("No canvas to present to: {}", err))
        .ok();

    let debug_overlay = DebugOverlay::new()
        .inspect_err(|err| tracing::warn!("No debug overlay: {:?}", err))
        .ok();

    let application = Rc::new(RefCell::new(Application {
        runtime,
        local_set,
        core,
        display,
        debug_overlay,
        picked_roms,
    }));

    if let Err(err) = rom_picker::register_open_rom_button(&application, picked_roms_sender) {
        tracing::warn!("No ROM picker: {:?}", err);
    }

    if let Err(err) = input::register_keyboard(&application) {
        tracing::warn!("No keyboard input: {:?}", err);
    }

    if let Err(err) = frame_loop::start_frame_loop(&application) {
        tracing::warn!("No frame loop: {:?}", err);
    }

    Ok(())
}
