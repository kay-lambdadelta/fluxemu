use std::{
    sync::{Arc, Mutex},
    time::Instant,
};

use egui::{Context, ViewportId};
use fluxemu_environment::Environment;
use fluxemu_frontend::{
    graphics::{DisplayContext, GraphicsRuntime, ProducableGraphicsRuntime},
    machine::FactoryManager,
};
use fluxemu_frontend_egui::{
    Frontend,
    rendering::{DrawTarget, EguiCapableGraphicsRuntime},
};
use fluxemu_input::{InputId, InputState, physical::PhysicalInputDeviceId};
use fluxemu_program::{ProgramManager, RomId};
use fluxemu_runtime::graphics::GraphicsRequirements;
use nalgebra::Vector2;
use palette::named::BLACK;
use raw_window_handle::{
    DisplayHandle, HandleError, HasDisplayHandle, HasWindowHandle, WindowHandle,
};
use winit::{
    application::ApplicationHandler,
    event::{ElementState, StartCause, WindowEvent},
    event_loop::{ActiveEventLoop, ControlFlow, EventLoop, EventLoopProxy},
    window::WindowId,
};

use crate::{
    audio::CpalAudioRuntime, event_loop::windowing::key::winit2key, font::load_fonts,
    gamepad::GamepadContext, platform::DesktopPlatform,
};

mod key;

pub fn run<R: ProducableGraphicsRuntime<Window> + EguiCapableGraphicsRuntime>(
    environment: Environment,
    program_manager: Arc<ProgramManager>,
    machine_factories: FactoryManager<DesktopPlatform<R, true>>,
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
        load_fonts(),
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

    let mut me = WindowingEventLoop {
        frontend,
        windowing_context: None,
        event_loop_proxy,
        refresh_surface: false,
        added_keyboard: false,
    };

    event_loop.run_app(&mut me)?;

    Ok(())
}

#[derive(Debug)]
enum Message {
    RedrawAt(Instant),
}

struct WindowingContext<R> {
    window: Window,
    egui_winit_context: egui_winit::State,
    graphics_runtime: R,
}

struct WindowingEventLoop<R: GraphicsRuntime> {
    windowing_context: Option<WindowingContext<R>>,
    frontend: Arc<Mutex<Frontend<DesktopPlatform<R, true>>>>,
    event_loop_proxy: EventLoopProxy<Message>,
    refresh_surface: bool,
    added_keyboard: bool,
}

impl<R: ProducableGraphicsRuntime<Window> + EguiCapableGraphicsRuntime> ApplicationHandler<Message>
    for WindowingEventLoop<R>
{
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        event_loop.set_control_flow(ControlFlow::Wait);
        let frontend = self.frontend.lock().unwrap();

        let window = Window::new(event_loop);
        let graphics_runtime = R::new(&window, GraphicsRequirements::default());
        let egui_context = frontend.egui_context();

        setup_egui_context(egui_context, self.event_loop_proxy.clone(), window.clone());

        let egui_winit_context = egui_winit::State::new(
            egui_context.clone(),
            ViewportId::ROOT,
            &window,
            Some(window.0.scale_factor() as f32),
            window.0.theme(),
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
            let response = egui_winit_context.on_window_event(&window.0, &event);

            // We have our own redrawing logic
            response.repaint && event != WindowEvent::RedrawRequested
        } else {
            true
        };

        if repaint {
            window.0.request_redraw();
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
                    let raw_input = egui_winit_context.take_egui_input(&window.0);
                    let full_output = frontend.run_menu(raw_input);

                    egui_winit_context
                        .handle_platform_output(&window.0, full_output.platform_output.clone());

                    graphics_runtime.present(
                        BLACK,
                        [DrawTarget::Gui {
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
                    Some(window.0.scale_factor() as f32),
                    window.0.theme(),
                    Some(graphics_runtime.max_texture_side() as usize),
                );

                let component_initialization_data =
                    graphics_runtime.component_initialization_data();

                // Immediately refresh since the backend changed
                window.0.request_redraw();

                component_initialization_data
            },
        );
    }

    fn exiting(&mut self, _event_loop: &ActiveEventLoop) {
        self.frontend.lock().unwrap().save_environment();
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
            window.0.request_redraw();
        }
    }
}

fn setup_egui_context(
    context: &Context,
    event_loop_proxy: EventLoopProxy<Message>,
    window: Window,
) {
    context.set_request_repaint_callback(move |info| {
        if info.delay.is_zero() {
            window.0.request_redraw();
        } else {
            let at = Instant::now() + info.delay;
            let _ = event_loop_proxy.send_event(Message::RedrawAt(at));
        }
    });
}

#[derive(Debug, Clone)]
pub struct Window(pub Arc<winit::window::Window>);

impl Window {
    pub fn new(event_loop: &ActiveEventLoop) -> Self {
        let window_attributes = winit::window::Window::default_attributes()
            .with_title("FluxEMU")
            .with_resizable(true)
            .with_transparent(false)
            .with_decorations(true);

        Window(Arc::new(
            event_loop.create_window(window_attributes).unwrap(),
        ))
    }
}

impl HasDisplayHandle for Window {
    fn display_handle(&self) -> Result<DisplayHandle<'_>, HandleError> {
        self.0.display_handle()
    }
}

impl HasWindowHandle for Window {
    fn window_handle(&self) -> Result<WindowHandle<'_>, HandleError> {
        self.0.window_handle()
    }
}

impl DisplayContext for Window {
    fn dimensions(&self) -> Vector2<u32> {
        let size = self.0.inner_size();

        Vector2::new(size.width, size.height)
    }

    fn pre_present_notify(&mut self) {
        self.0.pre_present_notify();
    }
}
