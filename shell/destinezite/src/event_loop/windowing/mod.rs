use std::{
    ops::Deref,
    sync::{Arc, Mutex},
    time::Instant,
};

use egui::{Context, ViewportId};
use fluxemu_environment::{ENVIRONMENT_LOCATION, Environment};
use fluxemu_frontend::{
    Frontend,
    graphics::{DrawTarget, GraphicsRuntime},
    machine::FactoryManager,
};
use fluxemu_input::{InputId, InputState, physical::PhysicalInputDeviceId};
use fluxemu_program::{ProgramManager, RomId};
use fluxemu_runtime::graphics::GraphicsRequirements;
use nalgebra::Vector2;
use palette::named::BLACK;
use ron::ser::PrettyConfig;
use winit::{
    application::ApplicationHandler,
    event::{ElementState, StartCause, WindowEvent},
    event_loop::{ActiveEventLoop, ControlFlow, EventLoop, EventLoopProxy},
    window::{Window, WindowId},
};

use crate::{
    audio::CpalAudioRuntime,
    display::{DisplayContext, RuntimeAssociatedDisplayContext},
    event_loop::windowing::key::winit2key,
    gamepad::GamepadContext,
    platform::DesktopPlatform,
};

mod key;

#[derive(Debug)]
enum Message {
    RedrawAt(Instant),
}

struct WindowingContext<R> {
    window: Arc<Window>,
    egui_winit_context: egui_winit::State,
    graphics_runtime: R,
}

pub struct WindowingEventLoop<R: GraphicsRuntime> {
    windowing_context: Option<WindowingContext<R>>,
    frontend: Arc<Mutex<Frontend<DesktopPlatform<R>>>>,
    event_loop_proxy: EventLoopProxy<Message>,
    refresh_surface: bool,
    added_keyboard: bool,
}

impl<R: GraphicsRuntime> WindowingEventLoop<R>
where
    Arc<Window>: RuntimeAssociatedDisplayContext<R>,
{
    pub fn run(
        environment: Environment,
        program_manager: Arc<ProgramManager>,
        machine_factories: FactoryManager<DesktopPlatform<R>>,
        initial_program: Option<Vec<RomId>>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let event_loop = EventLoop::with_user_event().build()?;
        let audio_runtime = CpalAudioRuntime::new().unwrap();

        let mut frontend = Frontend::new(
            environment,
            machine_factories,
            program_manager,
            audio_runtime,
            initial_program,
            true,
        );

        let gamepad_context = GamepadContext::new(&mut frontend);
        let frontend = Arc::new(Mutex::new(frontend));

        match gamepad_context {
            Ok(mut context) => {
                let frontend = frontend.clone();

                std::thread::Builder::new()
                    .name("Gamepad Poll Thread".to_string())
                    .spawn(move || {
                        loop {
                            if let Some(callback) = context.poll_gamepad_events(None) {
                                let mut frontend = frontend.lock().unwrap();
                                callback(&mut frontend);
                            }
                        }
                    })
                    .unwrap();
            }
            Err(err) => {
                tracing::error!(
                    "Gamepad context could not be created: {}, you will not have gamepad support",
                    err
                );
            }
        }

        let event_loop_proxy = event_loop.create_proxy();

        let mut me = Self {
            frontend,
            windowing_context: None,
            event_loop_proxy,
            refresh_surface: false,
            added_keyboard: false,
        };

        event_loop.run_app(&mut me)?;

        Ok(())
    }
}

impl<R: GraphicsRuntime> ApplicationHandler<Message> for WindowingEventLoop<R>
where
    Arc<Window>: RuntimeAssociatedDisplayContext<R>,
{
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        event_loop.set_control_flow(ControlFlow::Wait);
        let frontend = self.frontend.lock().unwrap();

        let window = setup_window(event_loop);
        let graphics_runtime = window.produce_runtime(GraphicsRequirements::default());
        let egui_context = frontend.egui_context();

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
        let mut frontend = self.frontend.lock().unwrap();

        // Pass events to egui if the frontend overlay is active
        let repaint = if frontend.overlay_active() {
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

                if frontend.overlay_active() {
                    let raw_input = egui_winit_context.take_egui_input(window);
                    let full_output = frontend.run_menu(raw_input);

                    egui_winit_context
                        .handle_platform_output(window, full_output.platform_output.clone());

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
            WindowEvent::KeyboardInput {
                event,
                is_synthetic,
                ..
            } => {
                if !self.added_keyboard {
                    frontend.register_gamepad(
                        PhysicalInputDeviceId::PLATFORM_RESERVED,
                        "Keyboard",
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
                    frontend.insert_input(
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
        let mut frontend = self.frontend.lock().unwrap();

        frontend.maybe_reset_graphics_to_meet_machine_requirements(
            |egui_context, sealed_machine_builder| {
                let WindowingContext {
                    window,
                    graphics_runtime,
                    egui_winit_context,
                } = self.windowing_context.as_mut().unwrap();
                setup_egui_context(egui_context, self.event_loop_proxy.clone(), window.clone());

                // Reconfigure graphics backend
                graphics_runtime.reconfigure(sealed_machine_builder.graphics_requirements());

                *egui_winit_context = egui_winit::State::new(
                    egui_context.clone(),
                    ViewportId::ROOT,
                    &window,
                    Some(window.scale_factor() as f32),
                    window.theme(),
                    Some(graphics_runtime.max_texture_side() as usize),
                );

                let component_initialization_data =
                    graphics_runtime.component_initialization_data();

                // Immediately refresh since the backend changed
                window.request_redraw();

                component_initialization_data
            },
        );
    }

    fn exiting(&mut self, _event_loop: &ActiveEventLoop) {
        let frontend = self.frontend.lock().unwrap();

        let environment_string =
            ron::ser::to_string_pretty(&frontend.environment, PrettyConfig::default()).unwrap();

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

impl DisplayContext for Arc<Window> {
    fn dimensions(&self) -> Vector2<u32> {
        let size = self.inner_size();

        Vector2::new(size.width, size.height)
    }

    fn pre_present_notify(&self) {
        Window::pre_present_notify(self);
    }
}
