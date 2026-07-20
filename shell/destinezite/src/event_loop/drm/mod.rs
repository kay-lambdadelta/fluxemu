use std::{
    borrow::Cow,
    collections::HashMap,
    marker::PhantomData,
    os::fd::AsFd,
    path::Path,
    sync::{Arc, Mutex},
    time::Instant,
};

use ::input::Libinput;
use drm::control::Device;
use egui::{RawInput, Rect, ViewportId, ViewportInfo};
use fluxemu_environment::Environment;
use fluxemu_frontend::{
    Frontend,
    graphics::{DrawTarget, GraphicsRuntime},
    machine::FactoryManager,
};
use fluxemu_program::{ProgramManager, RomId};
use fluxemu_runtime::graphics::GraphicsRequirements;
use libseat::{Seat, SeatEvent};
use nalgebra::Vector2;
use nix::{
    poll::PollTimeout,
    sys::epoll::{Epoll, EpollCreateFlags, EpollEvent, EpollFlags},
};
use palette::named::BLACK;

use crate::{
    audio::CpalAudioRuntime,
    display::{DisplayContext, RuntimeAssociatedDisplayContext},
    event_loop::drm::{
        card::{Card, DrmParams},
        input::{EguiInputCollector, Interface, build_xkb_state, handle_libinput_events},
    },
    gamepad::GamepadContext,
    platform::DesktopPlatform,
};

pub mod card;
mod input;

pub struct DrmEventLoop<R> {
    _phantom: PhantomData<fn() -> R>,
}

impl<R: GraphicsRuntime> DrmEventLoop<R>
where
    Arc<DrmContext>: RuntimeAssociatedDisplayContext<R>,
{
    pub fn run(
        environment: Environment,
        user_environment_location: Cow<'static, Path>,
        program_manager: Arc<ProgramManager>,
        machine_factories: FactoryManager<DesktopPlatform<R, false>>,
        initial_program: Option<Vec<RomId>>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        // Open the seat we need
        let mut seat = Seat::open(|seat, event| match event {
            SeatEvent::Enable => {}
            SeatEvent::Disable => {
                seat.disable().unwrap();
            }
        })?;

        let card_path = Card::find_suitable_card(&mut seat).expect("Could not find suitable card");
        let card = Card::open(card_path).expect("Could not open card");

        let context = Arc::new(DrmContext {
            params: card.select_suitable_params(),
            card,
        });
        let (width, height) = context.params.mode.size();

        // Set up our audio runtime so we can give it to the frontend
        let audio_runtime = CpalAudioRuntime::new().unwrap();

        // We can create the graphics runtime immediately (unlike with many window managers), it should also set up the DRM/KMS stuff for us
        let mut graphics_runtime = context.produce_runtime(GraphicsRequirements::default());

        let scale_factor = calculate_scale_factor(
            &context.card,
            &context.params.mode,
            context.params.connector_handle,
        );

        // Set up the input collector/translator and the frontend
        let mut frontend = Frontend::new(
            environment,
            user_environment_location,
            machine_factories,
            program_manager,
            audio_runtime,
            initial_program,
        );
        let gamepad_context = GamepadContext::new(&mut frontend);

        let egui_input_collector = EguiInputCollector::new(scale_factor);
        let frontend_state = Mutex::new(FrontendState {
            frontend,
            egui_input_collector,
        });

        // TODO: This "add keyboard conglomerate upon first key input" behavior is possibly not appropriate considering we can just use libinput
        // to figure out the keyboard story more directly
        let mut added_keyboard = false;

        // Enter the scope so we can share state with the gilrs thread without refcounting
        std::thread::scope(|scope| {
            let seat_name = seat.name();

            std::thread::Builder::new()
                .name("Input Poll Thread".to_string())
                .spawn_scoped(scope, || {
                    // Set up libinput for input reading and xkb for keyboard input interpretation
                    let mut libinput = Libinput::new_with_udev(Interface);
                    let mut xkb_state = build_xkb_state();

                    // Assign the seat
                    libinput.udev_assign_seat(seat_name).unwrap();

                    let epoll = Epoll::new(EpollCreateFlags::EPOLL_CLOEXEC).unwrap();

                    epoll
                        .add(libinput.as_fd(), EpollEvent::new(EpollFlags::EPOLLIN, 0))
                        .unwrap();

                    loop {
                        let mut ready_events = [EpollEvent::empty(); 1];

                        match epoll.wait(&mut ready_events, PollTimeout::NONE) {
                            Ok(num_events) if num_events > 0 => {
                                let mut frontend_state = frontend_state.lock().unwrap();
                                let FrontendState {
                                    frontend,
                                    egui_input_collector,
                                } = &mut *frontend_state;

                                for event in &ready_events[..num_events] {
                                    if event.data() == 0 {
                                        handle_libinput_events(
                                            frontend,
                                            egui_input_collector,
                                            &mut libinput,
                                            &mut xkb_state,
                                            &mut added_keyboard,
                                            width,
                                            height,
                                        );
                                    }
                                }
                            }
                            Err(err) => {
                                tracing::error!("Poll error: {}", err);
                            }
                            _ => {}
                        }
                    }
                })
                .unwrap();

            std::thread::Builder::new()
                .name("Gamepad Poll Thread".to_string())
                .spawn_scoped(scope, || match gamepad_context {
                    Ok(mut context) => loop {
                        if let Some(callback) = context.poll_gamepad_events(None) {
                            let mut frontend_state = frontend_state.lock().unwrap();
                            callback(&mut frontend_state.frontend);
                        }
                    },
                    Err(err) => {
                        tracing::error!(
                            "Gamepad context could not be created: {}, you will not have gamepad \
                             support",
                            err
                        );
                    }
                })
                .unwrap();

            let start_time = Instant::now();
            loop {
                let mut frontend_state_guard = frontend_state.lock().unwrap();
                let FrontendState {
                    frontend,
                    egui_input_collector,
                } = &mut *frontend_state_guard;

                // Possibly reset the graphics runtime
                frontend.maybe_reset_graphics_to_meet_machine_requirements(
                    |_, sealed_machine_builder| {
                        graphics_runtime
                            .reconfigure(sealed_machine_builder.graphics_requirements());
                        graphics_runtime.component_initialization_data()
                    },
                );

                // Make sure we actually drop the guard before presenting
                if frontend.overlay_active() {
                    let events = egui_input_collector.take_events();

                    let raw_input = RawInput {
                        viewport_id: ViewportId::ROOT,
                        viewports: HashMap::from_iter([(
                            ViewportId::ROOT,
                            ViewportInfo {
                                focused: Some(true),
                                fullscreen: Some(true),
                                native_pixels_per_point: Some(scale_factor),
                                ..Default::default()
                            },
                        )]),
                        screen_rect: Some(Rect {
                            min: [0.0, 0.0].into(),
                            max: [width as f32 / scale_factor, height as f32 / scale_factor].into(),
                        }),
                        modifiers: egui_input_collector.modifiers(),
                        time: Some(start_time.elapsed().as_secs_f64()),
                        focused: true,
                        events,
                        ..Default::default()
                    };

                    let full_output = frontend.run_menu(raw_input);
                    let context = frontend.egui_context().clone();

                    drop(frontend_state_guard);

                    graphics_runtime.present(
                        BLACK,
                        [DrawTarget::Egui {
                            context: &context,
                            full_output,
                        }],
                    );
                } else if let Some(machine) = frontend.machine() {
                    let machine = machine.clone();

                    drop(frontend_state_guard);

                    graphics_runtime.present(BLACK, [DrawTarget::Machine { machine: &machine }]);
                }
            }
        })
    }
}

pub fn mode_refresh_millihertz(mode: &drm::control::Mode) -> u32 {
    let clock = mode.clock() as u64;

    let (_, _, htotal) = mode.hsync();
    let (_, _, vtotal) = mode.vsync();

    let numerator = clock * 1_000_000;
    let denominator = htotal as u64 * vtotal as u64;

    ((numerator + denominator / 2) / denominator) as u32
}

fn calculate_scale_factor(
    card: &Card,
    mode: &drm::control::Mode,
    handle: drm::control::connector::Handle,
) -> f32 {
    let connector = card.get_connector(handle, false).unwrap();

    let Some((width, height)) = connector.size() else {
        // No way we could estimate scaling without physical dimensions
        return 1.0;
    };
    let physical_dimensions = Vector2::new(width, height);

    let (width, height) = mode.size();
    let pixel_dimensions = Vector2::new(width, height);

    crate::display::calculate_scale_factor(pixel_dimensions.cast(), physical_dimensions.cast())
}

struct FrontendState<R: GraphicsRuntime> {
    frontend: Frontend<DesktopPlatform<R, false>>,
    egui_input_collector: EguiInputCollector,
}

#[derive(Debug)]
pub struct DrmContext {
    pub card: Card,
    pub params: DrmParams,
}

impl DisplayContext for Arc<DrmContext> {
    fn dimensions(&self) -> Vector2<u32> {
        let (width, height) = self.params.mode.size();

        Vector2::new(width as u32, height as u32)
    }
}
