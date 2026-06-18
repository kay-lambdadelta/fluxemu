use std::{
    collections::HashMap,
    marker::PhantomData,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
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
use gilrs::{Gilrs, GilrsBuilder};
use libseat::{Seat, SeatEvent};
use nalgebra::Vector2;
use nix::{
    poll::PollTimeout,
    sys::{
        epoll::{Epoll, EpollCreateFlags, EpollEvent, EpollFlags},
        eventfd::{EfdFlags, EventFd},
    },
};
use palette::named::BLACK;

use crate::{
    audio::CpalAudioRuntime,
    display::{DisplayContext, RuntimeAssociatedDisplayContext},
    event_loop::drm::{
        card::{Card, DrmParams},
        input::{
            EguiInputCollector, Interface, build_xkb_state, handle_gilrs_events,
            handle_libinput_events,
        },
    },
    platform::DesktopPlatform,
};

pub mod card;
mod input;

pub struct DrmEventLoop<R> {
    _phantom: PhantomData<fn() -> R>,
}

impl<R: GraphicsRuntime> DrmEventLoop<R>
where
    for<'a> Arc<DrmContext>: RuntimeAssociatedDisplayContext<R, ProduceDataArgs<'a> = &'a mut Seat>,
{
    pub fn run(
        environment: Environment,
        program_manager: Arc<ProgramManager>,
        machine_factories: FactoryManager<DesktopPlatform<R>>,
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

        // This is being used as a waker for gilrs events
        let gilrs_event_fd = EventFd::from_value_and_flags(0, EfdFlags::EFD_CLOEXEC)?;

        let gilrs_state = Mutex::new(GilrsState {
            context: GilrsBuilder::new().build().unwrap(),
            peeked_event: None,
        });

        let threads_should_exit = AtomicBool::new(false);

        // Set up our audio runtime so we can give it to the frontend
        let audio_runtime = CpalAudioRuntime::new().unwrap();

        // We can create the graphics runtime immediately (unlike with many window managers), it should also set up the DRM/KMS stuff for us
        let mut graphics_runtime =
            context.produce_runtime(GraphicsRequirements::default(), &mut seat);

        let scale_factor = calculate_scale_factor(
            &context.card,
            &context.params.mode,
            context.params.connector_handle,
        );

        // Set up the input collector/translator and the frontend
        let frontend = Frontend::new(
            environment,
            machine_factories,
            program_manager,
            audio_runtime,
            initial_program,
            // No window manager, no external file dialog
            false,
        );
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
            std::thread::Builder::new()
                .name("Gilrs Gamepad Poller".to_string())
                .spawn_scoped(scope, || {
                    while !threads_should_exit.load(Ordering::Acquire) {
                        let mut had_events = false;

                        let mut gilrs_state_guard = gilrs_state.lock().unwrap();

                        // Scan for a event
                        //
                        // We have a timeout so exiting works as expected
                        if let Some(event) = gilrs_state_guard
                            .context
                            .next_event_blocking(Some(Duration::from_millis(1)))
                        {
                            gilrs_state_guard.peeked_event = Some(event);
                            had_events = true;
                        }
                        drop(gilrs_state_guard);

                        // Only wake up the event loop if anything actually changed
                        if had_events {
                            gilrs_event_fd.write(1).unwrap();
                        }
                    }
                })
                .unwrap();

            let seat_name = seat.name();
            std::thread::Builder::new()
                .name("Input Handler".to_string())
                .spawn_scoped(scope, || {
                    // Set up libinput for input reading and xkb for keyboard input interpretation
                    let mut libinput = Libinput::new_with_udev(Interface);
                    let mut xkb_state = build_xkb_state();

                    // Assign the seat
                    libinput.udev_assign_seat(seat_name).unwrap();

                    let epoll = make_epoll(&libinput, &gilrs_event_fd).unwrap();

                    while !threads_should_exit.load(Ordering::Acquire) {
                        let mut ready_events = [EpollEvent::empty(); 2];

                        match epoll.wait::<PollTimeout>(
                            &mut ready_events,
                            // Wake up sometimes in order to poll the should exit flag
                            Duration::from_millis(1).try_into().unwrap(),
                        ) {
                            Ok(num_events) if num_events > 0 => {
                                let mut frontend_state = frontend_state.lock().unwrap();
                                let FrontendState {
                                    frontend,
                                    egui_input_collector,
                                } = &mut *frontend_state;

                                for event in &ready_events[..num_events] {
                                    match event.data() {
                                        0 => {
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
                                        1 => {
                                            let mut gilrs_state_guard = gilrs_state.lock().unwrap();
                                            gilrs_event_fd.read().unwrap();

                                            handle_gilrs_events(frontend, &mut gilrs_state_guard);
                                            drop(gilrs_state_guard);
                                        }
                                        _ => {}
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

fn make_epoll(libinput: &Libinput, gilrs_event_fd: &EventFd) -> Result<Epoll, nix::Error> {
    let epoll = Epoll::new(EpollCreateFlags::EPOLL_CLOEXEC)?;

    epoll.add(libinput, EpollEvent::new(EpollFlags::EPOLLIN, 0))?;
    epoll.add(gilrs_event_fd, EpollEvent::new(EpollFlags::EPOLLIN, 1))?;

    Ok(epoll)
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

struct GilrsState {
    context: Gilrs,
    peeked_event: Option<gilrs::Event>,
}

struct FrontendState<R: GraphicsRuntime> {
    frontend: Frontend<DesktopPlatform<R>>,
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
