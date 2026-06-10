use std::{borrow::Cow, collections::HashMap, ops::Deref, sync::Arc, time::Instant};

use egui::{Context, ViewportId};
use fluxemu_environment::{ENVIRONMENT_LOCATION, Environment};
use fluxemu_frontend::{
    Frontend, PhysicalInputDeviceMetadata, graphics::GraphicsRuntime, machine::FactoryManager,
};
use fluxemu_input::{InputId, InputState, KeyboardInputId, physical::PhysicalInputDeviceId};
use fluxemu_program::{ProgramManager, RomId};
use fluxemu_runtime::graphics::GraphicsRequirements;
use gilrs::{Gilrs, GilrsBuilder};
use ron::ser::PrettyConfig;
use strum::IntoEnumIterator;
use uuid::Uuid;
use winit::{
    application::ApplicationHandler,
    event::{ElementState, StartCause, WindowEvent},
    event_loop::{ActiveEventLoop, ControlFlow, EventLoop, EventLoopProxy},
    window::{Window, WindowId},
};

use crate::{audio::CpalAudioRuntime, input::keyboard::winit2key, platform::DesktopPlatform};

#[derive(Debug)]
enum Message {
    RedrawAt(Instant),
}

struct WindowingContext<R: WinitCompatibleGraphicsRuntime> {
    window: Arc<Window>,
    egui_winit_context: egui_winit::State,
    graphics_runtime: R,
}

pub struct DesktopEventLoop<R: WinitCompatibleGraphicsRuntime> {
    windowing_context: Option<WindowingContext<R>>,
    gilrs_context: Gilrs,
    non_stable_controller_identification: HashMap<gilrs::GamepadId, PhysicalInputDeviceId>,
    frontend: Frontend<DesktopPlatform<R>>,
    event_loop_proxy: EventLoopProxy<Message>,
    refresh_surface: bool,
    added_keyboard: bool,
}

impl<R: WinitCompatibleGraphicsRuntime> DesktopEventLoop<R> {
    pub fn run(
        environment: Environment,
        program_manager: Arc<ProgramManager>,
        machine_factories: FactoryManager<DesktopPlatform<R>>,
        initial_program: Option<Vec<RomId>>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let event_loop = EventLoop::with_user_event().build()?;
        let gilrs_context = GilrsBuilder::new().build().unwrap();
        let non_stable_controller_identification = HashMap::new();
        let audio_runtime = CpalAudioRuntime::new().unwrap();

        let frontend = Frontend::new(
            environment,
            machine_factories,
            program_manager,
            audio_runtime,
            initial_program,
        );

        let event_loop_proxy = event_loop.create_proxy();

        let mut me = Self {
            frontend,
            windowing_context: None,
            gilrs_context,
            non_stable_controller_identification,
            event_loop_proxy,
            refresh_surface: false,
            added_keyboard: false,
        };

        event_loop.run_app(&mut me)?;

        Ok(())
    }
}

impl<R: WinitCompatibleGraphicsRuntime> ApplicationHandler<Message> for DesktopEventLoop<R> {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        event_loop.set_control_flow(ControlFlow::Wait);

        let window = setup_window(event_loop);
        let graphics_runtime = R::new(window.clone(), Default::default());
        let egui_context = self.frontend.egui_context();

        setup_egui_context(egui_context, self.event_loop_proxy.clone(), window.clone());

        let egui_winit_context = egui_winit::State::new(
            egui_context.clone(),
            ViewportId::ROOT,
            &window,
            Some(window.scale_factor() as f32),
            window.theme(),
            Some(graphics_runtime.max_texture_side() as usize),
        );

        self.windowing_context = Some(WindowingContext {
            window,
            graphics_runtime,
            egui_winit_context,
        });
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        _window_id: WindowId,
        event: WindowEvent,
    ) {
        let WindowingContext {
            window,
            graphics_runtime,
            egui_winit_context,
        } = self.windowing_context.as_mut().unwrap();

        // Pass events to egui if the frontend overlay is active
        let repaint = if self.frontend.frontend_overlay_active() {
            let response = egui_winit_context.on_window_event(window, &event);

            // We have our own redrawing logic
            response.repaint && event != WindowEvent::RedrawRequested
        } else {
            true
        };

        if repaint {
            window.request_redraw();
        }

        match event {
            WindowEvent::Resized(_) => {
                self.refresh_surface = true;
            }
            WindowEvent::RedrawRequested => {
                if std::mem::take(&mut self.refresh_surface) {
                    graphics_runtime.refresh_surface();
                }

                if self.frontend.frontend_overlay_active() {
                    let full_output = self
                        .frontend
                        .run_menu(egui_winit_context.take_egui_input(window));

                    egui_winit_context
                        .handle_platform_output(window, full_output.platform_output.clone());

                    graphics_runtime
                        .present_egui_overlay(self.frontend.egui_context(), full_output);
                } else if let Some(machine) = self.frontend.machine() {
                    graphics_runtime.present_machine(machine)
                }
            }
            WindowEvent::KeyboardInput {
                event,
                is_synthetic,
                ..
            } => {
                if !self.added_keyboard {
                    self.frontend.add_input_device(
                        PhysicalInputDeviceId::PLATFORM_RESERVED,
                        PhysicalInputDeviceMetadata {
                            name: Cow::Borrowed("Keyboard"),
                            present_inputs: KeyboardInputId::iter()
                                .map(InputId::Keyboard)
                                .collect(),
                        },
                        true,
                        // egui winit takes care of this for us
                        false,
                    );

                    self.added_keyboard = true;
                }

                if !is_synthetic
                    && !event.repeat
                    && let Some(key) = winit2key(event.physical_key)
                {
                    self.frontend.insert_input(
                        PhysicalInputDeviceId::PLATFORM_RESERVED,
                        InputId::Keyboard(key),
                        if event.state == ElementState::Pressed {
                            InputState::PRESSED
                        } else {
                            InputState::RELEASED
                        },
                    );
                }
            }
            WindowEvent::CloseRequested => {
                event_loop.exit();
            }
            _ => {}
        }
    }

    fn about_to_wait(&mut self, _event_loop: &ActiveEventLoop) {
        self.frontend.reset_graphics_to_meet_machine_requirements(
            |egui_context, sealed_machine_builder| {
                let WindowingContext {
                    window,
                    graphics_runtime,
                    ..
                } = self.windowing_context.take().unwrap();

                setup_egui_context(egui_context, self.event_loop_proxy.clone(), window.clone());

                // Destroy old graphics context
                drop(graphics_runtime);

                let graphics_runtime = R::new(
                    window.clone(),
                    sealed_machine_builder.graphics_requirements(),
                );

                let egui_winit_context = egui_winit::State::new(
                    egui_context.clone(),
                    ViewportId::ROOT,
                    &window,
                    Some(window.scale_factor() as f32),
                    window.theme(),
                    Some(graphics_runtime.max_texture_side() as usize),
                );

                let component_initialization_data =
                    graphics_runtime.component_initialization_data();

                // Make sure the user sees the game as it loads immediately
                window.request_redraw();

                self.windowing_context = Some(WindowingContext {
                    egui_winit_context,
                    window,
                    graphics_runtime,
                });

                component_initialization_data
            },
        );
    }

    fn exiting(&mut self, _event_loop: &ActiveEventLoop) {
        let environment_string =
            ron::ser::to_string_pretty(&self.frontend.environment, PrettyConfig::default())
                .unwrap();

        if let Err(error) = std::fs::write(ENVIRONMENT_LOCATION.deref(), environment_string) {
            tracing::error!("Failed to write environment file: {}", error);
        }
    }

    fn user_event(&mut self, event_loop: &ActiveEventLoop, event: Message) {
        match event {
            Message::RedrawAt(at) => {
                event_loop.set_control_flow(ControlFlow::WaitUntil(at));
            }
        }
    }

    fn new_events(&mut self, event_loop: &ActiveEventLoop, cause: StartCause) {
        // Timer ran out. Redraw and return to waiting
        if let StartCause::ResumeTimeReached { .. } = cause {
            let WindowingContext { window, .. } = self.windowing_context.as_mut().unwrap();

            event_loop.set_control_flow(ControlFlow::Wait);
            window.request_redraw();
        }
    }
}

fn produce_id_for_gilrs_gamepad(
    non_stable_controller_identification: &mut HashMap<gilrs::GamepadId, Uuid>,
    gilrs_gamepad_id: gilrs::GamepadId,
    gilrs_gamepad: gilrs::Gamepad<'_>,
) -> PhysicalInputDeviceId {
    let mut gamepad_id = Uuid::from_bytes(gilrs_gamepad.uuid());
    if gamepad_id == Uuid::nil() {
        gamepad_id = *non_stable_controller_identification
            .entry(gilrs_gamepad_id)
            .or_insert_with(|| {
                tracing::warn!(
                    "Gamepad {} is not giving us an ID, assigning it a arbitary one",
                    gamepad_id
                );

                Uuid::new_v4()
            });
    }

    PhysicalInputDeviceId::new(gamepad_id.try_into().unwrap())
}

fn setup_egui_context(
    context: &Context,
    event_loop_proxy: EventLoopProxy<Message>,
    window: Arc<Window>,
) {
    context.set_request_repaint_callback(move |info| {
        if info.delay.is_zero() {
            window.request_redraw();
        } else {
            let at = Instant::now() + info.delay;
            let _ = event_loop_proxy.send_event(Message::RedrawAt(at));
        }
    });
}

fn setup_window(event_loop: &ActiveEventLoop) -> Arc<Window> {
    let window_attributes = Window::default_attributes()
        .with_title("FluxEMU")
        .with_resizable(true)
        .with_transparent(false)
        .with_decorations(true);

    Arc::new(event_loop.create_window(window_attributes).unwrap())
}

pub trait WinitCompatibleGraphicsRuntime: GraphicsRuntime {
    fn new(window: Arc<Window>, requirements: GraphicsRequirements<Self::GraphicsApi>) -> Self;
}
