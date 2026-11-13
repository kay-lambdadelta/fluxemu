use std::{
    collections::HashMap,
    os::fd::AsFd,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
        mpsc::channel,
    },
    time::Instant,
};

use ::input::Libinput;
use drm::control::Device;
use egui::{RawInput, Rect, ViewportId, ViewportInfo};
use fluxemu_database::ProgramDatabase;
use fluxemu_environment::Environment;
use fluxemu_frontend::{
    FrontendCore,
    graphics::{DisplayContext, ProducableGraphicsRuntime},
    machine::FactoryManager,
    notification::NotificationReceiver,
};
use fluxemu_frontend_egui::{Frontend, rendering::EguiCapableGraphicsRuntime};
use fluxemu_program::{ProgramManager, RomId};
use fluxemu_runtime::graphics::GraphicsRequirements;
use nalgebra::Vector2;
use nix::{
    poll::PollTimeout,
    sys::epoll::{Epoll, EpollCreateFlags, EpollEvent, EpollFlags},
};
use palette::named::BLACK;
use tokio::runtime::Handle;

use crate::{
    audio::CpalAudioRuntime,
    event_loop::drm::{
        card::{Card, DrmParams},
        input::{EguiInputCollector, Interface, build_xkb_state, handle_libinput_events},
        seat::{PresentationGate, handle_seat_events, open_seat},
    },
    font::load_fonts,
    gamepad::{GamepadContext, InputMessageSink},
    platform::DesktopPlatform,
};

pub mod card;
mod input;
mod seat;

const LIBINPUT_EVENT_SOURCE: u64 = 0;
const SEAT_EVENT_SOURCE: u64 = 1;

pub fn run<R: ProducableGraphicsRuntime<DrmContext> + EguiCapableGraphicsRuntime>(
    runtime: Handle,
    environment: Environment,
    program_manager: Arc<ProgramManager>,
    program_database: Arc<ProgramDatabase>,
    machine_factories: FactoryManager<DesktopPlatform<R, false>>,
    initial_program: Option<Vec<RomId>>,
    notifications: NotificationReceiver,
) -> Result<(), Box<dyn std::error::Error>> {
    // Open the seat we need
    let (seat, seat_events) = open_seat()?;

    let card = {
        let mut seat = seat.lock().unwrap();
        let card_path = Card::find_suitable_card(&mut seat).expect("Could not find suitable card");

        Card::open(&mut seat, card_path).expect("Could not open card")
    };

    let context = Arc::new(DrmContext {
        params: card.select_suitable_params(),
        card: Arc::new(card),
    });
    let (width, height) = context.params.mode.size();

    // Set up our audio runtime so we can give it to the frontend
    let audio_runtime = CpalAudioRuntime::new().unwrap();

    // We can create the graphics runtime immediately (unlike with many window managers), it should also set up the DRM/KMS stuff for us
    let mut graphics_runtime = R::new(&context, GraphicsRequirements::default());

    let scale_factor = calculate_scale_factor(
        &context.card,
        &context.params.mode,
        context.params.connector_handle,
    );

    // Set up the input collector/translator and the frontend
    let core = FrontendCore::new(
        runtime,
        environment,
        machine_factories,
        program_manager,
        program_database,
        audio_runtime,
        initial_program,
    );
    let mut frontend = Frontend::new(core, notifications, load_fonts());
    let gamepad_context = GamepadContext::new(&mut frontend);

    let egui_input_collector = Mutex::new(EguiInputCollector::new(scale_factor));
    let machine_has_focus = AtomicBool::new(false);
    let (input_sender, input_receiver) = channel();

    // TODO: This "add keyboard conglomerate upon first key input" behavior is possibly not appropriate considering we can just use libinput
    // to figure out the keyboard story more directly
    let mut added_keyboard = false;
    let presentation_gate = PresentationGate::default();

    // Enter the scope so we can share state with the gilrs thread without refcounting
    let libinput_sender = input_sender.clone();

    std::thread::scope(|scope| {
        std::thread::Builder::new()
            .name("Input Poll Thread".to_string())
            .spawn_scoped(scope, || {
                let seat_events = seat_events;
                let mut send_input = |message| {
                    let _ = libinput_sender.send(message);
                };

                // Set up libinput for input reading and xkb for keyboard input interpretation
                let mut libinput = Libinput::new_with_udev(Interface::new(seat.clone()));
                let mut xkb_state = build_xkb_state();

                // Assign the seat
                let seat_name = seat.lock().unwrap().name().to_owned();
                libinput.udev_assign_seat(&seat_name).unwrap();

                let epoll = Epoll::new(EpollCreateFlags::EPOLL_CLOEXEC).unwrap();

                epoll
                    .add(
                        libinput.as_fd(),
                        EpollEvent::new(EpollFlags::EPOLLIN, LIBINPUT_EVENT_SOURCE),
                    )
                    .unwrap();

                epoll
                    .add(
                        seat.lock().unwrap().get_fd().unwrap(),
                        EpollEvent::new(EpollFlags::EPOLLIN, SEAT_EVENT_SOURCE),
                    )
                    .unwrap();

                loop {
                    let mut ready_events = [EpollEvent::empty(); 2];

                    match epoll.wait(&mut ready_events, PollTimeout::NONE) {
                        Ok(num_events) => {
                            for event in &ready_events[..num_events] {
                                match event.data() {
                                    LIBINPUT_EVENT_SOURCE => {
                                        handle_libinput_events(
                                            &mut send_input,
                                            machine_has_focus.load(Ordering::Relaxed),
                                            &mut egui_input_collector.lock().unwrap(),
                                            &mut libinput,
                                            &mut xkb_state,
                                            &mut added_keyboard,
                                            width,
                                            height,
                                        );
                                    }
                                    SEAT_EVENT_SOURCE => {
                                        handle_seat_events(
                                            &seat,
                                            &seat_events,
                                            &mut libinput,
                                            &presentation_gate,
                                        );
                                    }
                                    _ => {}
                                }
                            }
                        }
                        Err(err) => {
                            tracing::error!("Poll error: {}", err);
                        }
                    }
                }
            })
            .unwrap();

        std::thread::Builder::new()
            .name("Gamepad Poll Thread".to_string())
            .spawn_scoped(scope, move || match gamepad_context {
                Ok(mut context) => {
                    let mut sink = InputMessageSink(|message| {
                        let _ = input_sender.send(message);
                    });

                    loop {
                        if let Some(callback) = context.poll_gamepad_events(None) {
                            callback(&mut sink);
                        }
                    }
                }
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
            if presentation_gate.begin_frame() {
                graphics_runtime.refresh_surface();
            }

            for message in input_receiver.try_iter() {
                message.apply(&mut frontend);
            }

            // Possibly reset the graphics runtime
            frontend.maybe_reset_graphics_to_meet_machine_requirements(
                |_, sealed_machine_builder| {
                    graphics_runtime.reconfigure(sealed_machine_builder.graphics_requirements());
                },
            );

            let events = egui_input_collector.lock().unwrap().take_events();

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
                time: Some(start_time.elapsed().as_secs_f64()),
                focused: true,
                events,
                ..Default::default()
            };

            let full_output = frontend.run(raw_input, &mut graphics_runtime);
            machine_has_focus.store(frontend.machine_has_focus(), Ordering::Relaxed);

            graphics_runtime.present(frontend.egui_context(), BLACK, full_output);
            presentation_gate.end_frame();
        }
    })
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

#[derive(Clone, Debug)]
pub struct DrmContext {
    pub card: Arc<Card>,
    pub params: DrmParams,
}

impl DisplayContext for DrmContext {
    fn dimensions(&self) -> Vector2<u32> {
        let (width, height) = self.params.mode.size();

        Vector2::new(width as u32, height as u32)
    }
}
