use std::{borrow::Cow, collections::HashMap, ops::Deref, sync::Arc, time::Instant};

use egui::{Context, ViewportId};
use fluxemu_environment::{ENVIRONMENT_LOCATION, Environment};
use fluxemu_frontend::{
    Frontend, PhysicalInputDeviceMetadata,
    graphics::{DrawTarget, GraphicsRuntime},
    machine::FactoryManager,
};
use fluxemu_input::{InputId, InputState, KeyboardInputId, physical::PhysicalInputDeviceId};
use fluxemu_program::{ProgramManager, RomId};
use fluxemu_runtime::graphics::GraphicsRequirements;
use gilrs::{Gilrs, GilrsBuilder};
use nalgebra::Vector2;
use palette::named::BLACK;
use ron::ser::PrettyConfig;
use strum::IntoEnumIterator;
use winit::{
    application::ApplicationHandler,
    event::{ElementState, StartCause, WindowEvent},
    event_loop::{ActiveEventLoop, ControlFlow, EventLoop, EventLoopProxy},
    keyboard::{KeyCode, PhysicalKey},
    window::{Window, WindowId},
};

use crate::{
    audio::CpalAudioRuntime,
    display::{DisplayContext, RuntimeAssociatedDisplayContext},
    platform::DesktopPlatform,
};

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
    gilrs_context: Gilrs,
    non_stable_controller_identification: HashMap<gilrs::GamepadId, PhysicalInputDeviceId>,
    frontend: Frontend<DesktopPlatform<R>>,
    event_loop_proxy: EventLoopProxy<Message>,
    refresh_surface: bool,
    added_keyboard: bool,
}

impl<'a, R: GraphicsRuntime> WindowingEventLoop<R>
where
    Arc<Window>: RuntimeAssociatedDisplayContext<R, ProduceDataArgs<'a> = ()>,
{
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
            true,
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

impl<'a, R: GraphicsRuntime> ApplicationHandler<Message> for WindowingEventLoop<R>
where
    Arc<Window>: RuntimeAssociatedDisplayContext<R, ProduceDataArgs<'a> = ()>,
{
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        event_loop.set_control_flow(ControlFlow::Wait);

        let window = setup_window(event_loop);
        let graphics_runtime = window.produce_runtime(GraphicsRequirements::default(), ());
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
        let repaint = if self.frontend.overlay_active() {
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

                if self.frontend.overlay_active() {
                    let raw_input = egui_winit_context.take_egui_input(window);
                    let full_output = self.frontend.run_menu(raw_input);

                    egui_winit_context
                        .handle_platform_output(window, full_output.platform_output.clone());

                    graphics_runtime.present(
                        BLACK,
                        [DrawTarget::Egui {
                            context: self.frontend.egui_context(),
                            full_output,
                        }],
                    );
                } else if let Some(machine) = self.frontend.machine() {
                    graphics_runtime.present(BLACK, [DrawTarget::Machine { machine }]);
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
        self.frontend
            .maybe_reset_graphics_to_meet_machine_requirements(
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

pub fn winit2key(key: PhysicalKey) -> Option<KeyboardInputId> {
    Some(match key {
        PhysicalKey::Code(code) => match code {
            KeyCode::Backquote => KeyboardInputId::Backquote,
            KeyCode::Backslash => KeyboardInputId::Backslash,
            KeyCode::BracketLeft => KeyboardInputId::BracketLeft,
            KeyCode::BracketRight => KeyboardInputId::BracketRight,
            KeyCode::Comma => KeyboardInputId::Comma,
            KeyCode::Digit0 => KeyboardInputId::Digit0,
            KeyCode::Digit1 => KeyboardInputId::Digit1,
            KeyCode::Digit2 => KeyboardInputId::Digit2,
            KeyCode::Digit3 => KeyboardInputId::Digit3,
            KeyCode::Digit4 => KeyboardInputId::Digit4,
            KeyCode::Digit5 => KeyboardInputId::Digit5,
            KeyCode::Digit6 => KeyboardInputId::Digit6,
            KeyCode::Digit7 => KeyboardInputId::Digit7,
            KeyCode::Digit8 => KeyboardInputId::Digit8,
            KeyCode::Digit9 => KeyboardInputId::Digit9,
            KeyCode::Equal => KeyboardInputId::Equal,
            KeyCode::IntlBackslash => KeyboardInputId::IntlBackslash,
            KeyCode::IntlRo => KeyboardInputId::IntlRo,
            KeyCode::IntlYen => KeyboardInputId::IntlYen,
            KeyCode::KeyA => KeyboardInputId::KeyA,
            KeyCode::KeyB => KeyboardInputId::KeyB,
            KeyCode::KeyC => KeyboardInputId::KeyC,
            KeyCode::KeyD => KeyboardInputId::KeyD,
            KeyCode::KeyE => KeyboardInputId::KeyE,
            KeyCode::KeyF => KeyboardInputId::KeyF,
            KeyCode::KeyG => KeyboardInputId::KeyG,
            KeyCode::KeyH => KeyboardInputId::KeyH,
            KeyCode::KeyI => KeyboardInputId::KeyI,
            KeyCode::KeyJ => KeyboardInputId::KeyJ,
            KeyCode::KeyK => KeyboardInputId::KeyK,
            KeyCode::KeyL => KeyboardInputId::KeyL,
            KeyCode::KeyM => KeyboardInputId::KeyM,
            KeyCode::KeyN => KeyboardInputId::KeyN,
            KeyCode::KeyO => KeyboardInputId::KeyO,
            KeyCode::KeyP => KeyboardInputId::KeyP,
            KeyCode::KeyQ => KeyboardInputId::KeyQ,
            KeyCode::KeyR => KeyboardInputId::KeyR,
            KeyCode::KeyS => KeyboardInputId::KeyS,
            KeyCode::KeyT => KeyboardInputId::KeyT,
            KeyCode::KeyU => KeyboardInputId::KeyU,
            KeyCode::KeyV => KeyboardInputId::KeyV,
            KeyCode::KeyW => KeyboardInputId::KeyW,
            KeyCode::KeyX => KeyboardInputId::KeyX,
            KeyCode::KeyY => KeyboardInputId::KeyY,
            KeyCode::KeyZ => KeyboardInputId::KeyZ,
            KeyCode::Minus => KeyboardInputId::Minus,
            KeyCode::Period => KeyboardInputId::Period,
            KeyCode::Quote => KeyboardInputId::Quote,
            KeyCode::Semicolon => KeyboardInputId::Semicolon,
            KeyCode::Slash => KeyboardInputId::Slash,
            KeyCode::AltLeft => KeyboardInputId::AltLeft,
            KeyCode::AltRight => KeyboardInputId::AltRight,
            KeyCode::Backspace => KeyboardInputId::Backspace,
            KeyCode::CapsLock => KeyboardInputId::CapsLock,
            KeyCode::ContextMenu => KeyboardInputId::ContextMenu,
            KeyCode::ControlLeft => KeyboardInputId::ControlLeft,
            KeyCode::ControlRight => KeyboardInputId::ControlRight,
            KeyCode::Enter => KeyboardInputId::Enter,
            KeyCode::SuperLeft => KeyboardInputId::SuperLeft,
            KeyCode::SuperRight => KeyboardInputId::SuperRight,
            KeyCode::ShiftLeft => KeyboardInputId::ShiftLeft,
            KeyCode::ShiftRight => KeyboardInputId::ShiftRight,
            KeyCode::Space => KeyboardInputId::Space,
            KeyCode::Tab => KeyboardInputId::Tab,
            KeyCode::Convert => KeyboardInputId::Convert,
            KeyCode::KanaMode => KeyboardInputId::KanaMode,
            KeyCode::Lang1 => KeyboardInputId::Lang1,
            KeyCode::Lang2 => KeyboardInputId::Lang2,
            KeyCode::Lang3 => KeyboardInputId::Lang3,
            KeyCode::Lang4 => KeyboardInputId::Lang4,
            KeyCode::Lang5 => KeyboardInputId::Lang5,
            KeyCode::NonConvert => KeyboardInputId::NonConvert,
            KeyCode::Delete => KeyboardInputId::Delete,
            KeyCode::End => KeyboardInputId::End,
            KeyCode::Help => KeyboardInputId::Help,
            KeyCode::Home => KeyboardInputId::Home,
            KeyCode::Insert => KeyboardInputId::Insert,
            KeyCode::PageDown => KeyboardInputId::PageDown,
            KeyCode::PageUp => KeyboardInputId::PageUp,
            KeyCode::ArrowDown => KeyboardInputId::ArrowDown,
            KeyCode::ArrowLeft => KeyboardInputId::ArrowLeft,
            KeyCode::ArrowRight => KeyboardInputId::ArrowRight,
            KeyCode::ArrowUp => KeyboardInputId::ArrowUp,
            KeyCode::NumLock => KeyboardInputId::NumLock,
            KeyCode::Numpad0 => KeyboardInputId::Numpad0,
            KeyCode::Numpad1 => KeyboardInputId::Numpad1,
            KeyCode::Numpad2 => KeyboardInputId::Numpad2,
            KeyCode::Numpad3 => KeyboardInputId::Numpad3,
            KeyCode::Numpad4 => KeyboardInputId::Numpad4,
            KeyCode::Numpad5 => KeyboardInputId::Numpad5,
            KeyCode::Numpad6 => KeyboardInputId::Numpad6,
            KeyCode::Numpad7 => KeyboardInputId::Numpad7,
            KeyCode::Numpad8 => KeyboardInputId::Numpad8,
            KeyCode::Numpad9 => KeyboardInputId::Numpad9,
            KeyCode::NumpadAdd => KeyboardInputId::NumpadAdd,
            KeyCode::NumpadBackspace => KeyboardInputId::NumpadBackspace,
            KeyCode::NumpadClear => KeyboardInputId::NumpadClear,
            KeyCode::NumpadClearEntry => KeyboardInputId::NumpadClearEntry,
            KeyCode::NumpadComma => KeyboardInputId::NumpadComma,
            KeyCode::NumpadDecimal => KeyboardInputId::NumpadDecimal,
            KeyCode::NumpadDivide => KeyboardInputId::NumpadDivide,
            KeyCode::NumpadEnter => KeyboardInputId::NumpadEnter,
            KeyCode::NumpadEqual => KeyboardInputId::NumpadEqual,
            KeyCode::NumpadHash => KeyboardInputId::NumpadHash,
            KeyCode::NumpadMemoryAdd => KeyboardInputId::NumpadMemoryAdd,
            KeyCode::NumpadMemoryClear => KeyboardInputId::NumpadMemoryClear,
            KeyCode::NumpadMemoryRecall => KeyboardInputId::NumpadMemoryRecall,
            KeyCode::NumpadMemoryStore => KeyboardInputId::NumpadMemoryStore,
            KeyCode::NumpadMemorySubtract => KeyboardInputId::NumpadMemorySubtract,
            KeyCode::NumpadMultiply => KeyboardInputId::NumpadMultiply,
            KeyCode::NumpadParenLeft => KeyboardInputId::NumpadParenLeft,
            KeyCode::NumpadParenRight => KeyboardInputId::NumpadParenRight,
            KeyCode::NumpadStar => KeyboardInputId::NumpadStar,
            KeyCode::NumpadSubtract => KeyboardInputId::NumpadSubtract,
            KeyCode::Escape => KeyboardInputId::Escape,
            KeyCode::Fn => KeyboardInputId::Fn,
            KeyCode::FnLock => KeyboardInputId::FnLock,
            KeyCode::PrintScreen => KeyboardInputId::PrintScreen,
            KeyCode::ScrollLock => KeyboardInputId::ScrollLock,
            KeyCode::Pause => KeyboardInputId::Pause,
            KeyCode::BrowserBack => KeyboardInputId::BrowserBack,
            KeyCode::BrowserFavorites => KeyboardInputId::BrowserFavorites,
            KeyCode::BrowserForward => KeyboardInputId::BrowserForward,
            KeyCode::BrowserHome => KeyboardInputId::BrowserHome,
            KeyCode::BrowserRefresh => KeyboardInputId::BrowserRefresh,
            KeyCode::BrowserSearch => KeyboardInputId::BrowserSearch,
            KeyCode::BrowserStop => KeyboardInputId::BrowserStop,
            KeyCode::Eject => KeyboardInputId::Eject,
            KeyCode::LaunchApp1 => KeyboardInputId::LaunchApp1,
            KeyCode::LaunchApp2 => KeyboardInputId::LaunchApp2,
            KeyCode::LaunchMail => KeyboardInputId::LaunchMail,
            KeyCode::MediaPlayPause => KeyboardInputId::MediaPlayPause,
            KeyCode::MediaSelect => KeyboardInputId::MediaSelect,
            KeyCode::MediaStop => KeyboardInputId::MediaStop,
            KeyCode::MediaTrackNext => KeyboardInputId::MediaTrackNext,
            KeyCode::MediaTrackPrevious => KeyboardInputId::MediaTrackPrevious,
            KeyCode::Power => KeyboardInputId::Power,
            KeyCode::Sleep => KeyboardInputId::Sleep,
            KeyCode::AudioVolumeDown => KeyboardInputId::AudioVolumeDown,
            KeyCode::AudioVolumeMute => KeyboardInputId::AudioVolumeMute,
            KeyCode::AudioVolumeUp => KeyboardInputId::AudioVolumeUp,
            KeyCode::WakeUp => KeyboardInputId::WakeUp,
            KeyCode::Meta => KeyboardInputId::MetaLeft,
            KeyCode::Hyper => KeyboardInputId::Hyper,
            KeyCode::Turbo => KeyboardInputId::Turbo,
            KeyCode::Abort => KeyboardInputId::Abort,
            KeyCode::Resume => KeyboardInputId::Resume,
            KeyCode::Suspend => KeyboardInputId::Suspend,
            KeyCode::Again => KeyboardInputId::Again,
            KeyCode::Copy => KeyboardInputId::Copy,
            KeyCode::Cut => KeyboardInputId::Cut,
            KeyCode::Find => KeyboardInputId::Find,
            KeyCode::Open => KeyboardInputId::Open,
            KeyCode::Paste => KeyboardInputId::Paste,
            KeyCode::Props => KeyboardInputId::Props,
            KeyCode::Select => KeyboardInputId::Select,
            KeyCode::Undo => KeyboardInputId::Undo,
            KeyCode::Hiragana => KeyboardInputId::Hiragana,
            KeyCode::Katakana => KeyboardInputId::Katakana,
            KeyCode::F1 => KeyboardInputId::F1,
            KeyCode::F2 => KeyboardInputId::F2,
            KeyCode::F3 => KeyboardInputId::F3,
            KeyCode::F4 => KeyboardInputId::F4,
            KeyCode::F5 => KeyboardInputId::F5,
            KeyCode::F6 => KeyboardInputId::F6,
            KeyCode::F7 => KeyboardInputId::F7,
            KeyCode::F8 => KeyboardInputId::F8,
            KeyCode::F9 => KeyboardInputId::F9,
            KeyCode::F10 => KeyboardInputId::F10,
            KeyCode::F11 => KeyboardInputId::F11,
            KeyCode::F12 => KeyboardInputId::F12,
            KeyCode::F13 => KeyboardInputId::F13,
            KeyCode::F14 => KeyboardInputId::F14,
            KeyCode::F15 => KeyboardInputId::F15,
            KeyCode::F16 => KeyboardInputId::F16,
            KeyCode::F17 => KeyboardInputId::F17,
            KeyCode::F18 => KeyboardInputId::F18,
            KeyCode::F19 => KeyboardInputId::F19,
            KeyCode::F20 => KeyboardInputId::F20,
            KeyCode::F21 => KeyboardInputId::F21,
            KeyCode::F22 => KeyboardInputId::F22,
            KeyCode::F23 => KeyboardInputId::F23,
            KeyCode::F24 => KeyboardInputId::F24,
            KeyCode::F25 => KeyboardInputId::F25,
            KeyCode::F26 => KeyboardInputId::F26,
            KeyCode::F27 => KeyboardInputId::F27,
            KeyCode::F28 => KeyboardInputId::F28,
            KeyCode::F29 => KeyboardInputId::F29,
            KeyCode::F30 => KeyboardInputId::F30,
            KeyCode::F31 => KeyboardInputId::F31,
            KeyCode::F32 => KeyboardInputId::F32,
            KeyCode::F33 => KeyboardInputId::F33,
            KeyCode::F34 => KeyboardInputId::F34,
            KeyCode::F35 => KeyboardInputId::F35,
            _ => todo!(),
        },
        PhysicalKey::Unidentified(_) => return None,
    })
}
