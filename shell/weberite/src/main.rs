use std::{
    cell::RefCell,
    path::PathBuf,
    rc::Rc,
    sync::mpsc::{Receiver, channel},
};

use fluxemu_database::ProgramDatabase;
use fluxemu_environment::{Environment, graphics::GraphicsApi};
use fluxemu_frontend::{FrontendCore, FrontendEvent};
use fluxemu_program::{DigestCache, ProgramManager};
use fluxemu_runtime::graphics::GraphicsRequirements;
use redb::Database;
use tokio::{runtime::Runtime, task::LocalSet};
use tracing_subscriber::{filter::LevelFilter, layer::SubscriberExt, util::SubscriberInitExt};

use crate::{
    build_machine::get_factories,
    debug_overlay::DebugOverlay,
    display::{CanvasGraphicsRuntime, SoftwareGraphicsRuntime},
    platform::Platform,
    runtime::AudioRuntime,
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

struct Application<R: CanvasGraphicsRuntime> {
    runtime: Runtime,
    local_set: LocalSet,
    core: FrontendCore<Platform<R>>,
    graphics_runtime: R,
    debug_overlay: Option<DebugOverlay>,
    picked_roms: Receiver<PathBuf>,
}

impl<R: CanvasGraphicsRuntime> Application<R> {
    fn frame(&mut self) {
        self.runtime
            .block_on(self.local_set.run_until(tokio::task::yield_now()));

        let _local_set_guard = self.local_set.enter();

        let picked_roms: Vec<_> = self.picked_roms.try_iter().collect();
        if !picked_roms.is_empty() {
            self.core.load_roms(picked_roms);
        }

        self.core.poll(&mut self.graphics_runtime);

        if self.core.has_pending_machine() {
            let graphics_runtime = &mut self.graphics_runtime;

            self.core.start_pending_machine(|sealed_machine_builder| {
                graphics_runtime.reconfigure(sealed_machine_builder.graphics_requirements());
            });
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

        let shader = self.core.environment().graphics.shader;
        let graphics_runtime = &mut self.graphics_runtime;

        if let Some(Err(err)) = self.core.read_framebuffer(|framebuffer, display_geometry| {
            graphics_runtime.present(framebuffer, display_geometry, shader)
        }) {
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
    let canvas = display::find_canvas()?;

    match environment.graphics.api {
        GraphicsApi::Software => run(
            environment,
            SoftwareGraphicsRuntime::new(canvas, GraphicsRequirements::default())?,
        ),
        #[cfg(feature = "webgpu")]
        GraphicsApi::Webgpu => {
            match display::WebgpuGraphicsRuntime::new(
                canvas.clone(),
                GraphicsRequirements::default(),
            ) {
                Ok(graphics_runtime) => run(environment, graphics_runtime),
                Err(err) => {
                    tracing::error!(
                        "Could not initialize WebGPU, falling back to software: {}",
                        err
                    );

                    let canvas = display::replace_canvas(&canvas)?;
                    run(
                        environment,
                        SoftwareGraphicsRuntime::new(canvas, GraphicsRequirements::default())?,
                    )
                }
            }
        }
        graphics_api => {
            tracing::warn!(
                "{} is not available in this build, falling back to software",
                graphics_api
            );

            run(
                environment,
                SoftwareGraphicsRuntime::new(canvas, GraphicsRequirements::default())?,
            )
        }
    }
}

fn run<R: CanvasGraphicsRuntime>(
    environment: Environment,
    graphics_runtime: R,
) -> Result<(), Box<dyn std::error::Error>> {
    let runtime = Runtime::new()?;
    let program_database = ProgramDatabase::new(Database::create(&environment.database_location)?)?;
    let program_manager = ProgramManager::with_digest_cache(
        environment.rom_stores.clone(),
        DigestCache::try_open(&environment.digest_cache_location),
    );

    let local_set = LocalSet::new();

    let core = {
        let _local_set_guard = local_set.enter();

        FrontendCore::<Platform<R>>::new(
            runtime.handle().clone(),
            environment,
            get_factories(),
            program_manager,
            program_database,
            AudioRuntime,
            None,
        )
    };

    let (picked_roms_sender, picked_roms) = channel();

    let debug_overlay = DebugOverlay::new()
        .inspect_err(|err| tracing::warn!("No debug overlay: {:?}", err))
        .ok();

    let application = Rc::new(RefCell::new(Application {
        runtime,
        local_set,
        core,
        graphics_runtime,
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
