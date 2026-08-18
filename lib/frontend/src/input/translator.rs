use std::sync::Once;

use egui::{Context, Event, Key, Modifiers, PointerButton, ViewportId};
use fluxemu_input::{GamepadInputId, InputId, InputState, KeyboardInputId};
use fluxemu_math::rectangle::Rectangle;
use nalgebra::{Point2, Vector2};

const POINTER_SENSITIVITY: f32 = 2.0;

pub struct EguiInputTranslator {
    events: Vec<Event>,
    pointer_position: Point2<f32>,
    screen_rectangle: Option<Rectangle<f32>>,
}

impl Default for EguiInputTranslator {
    fn default() -> Self {
        Self {
            events: Vec::new(),
            pointer_position: Point2::origin(),
            screen_rectangle: None,
        }
    }
}

impl EguiInputTranslator {
    pub fn insert_input(&mut self, egui_context: &Context, input_id: InputId, state: InputState) {
        let rectangle = egui_context.input_for(ViewportId::ROOT, |state| {
            let rectangle = state.content_rect();

            Rectangle::from_min_and_max(
                Point2::new(rectangle.min.x, rectangle.min.y),
                Point2::new(rectangle.max.x, rectangle.max.y),
            )
        });
        self.screen_rectangle = Some(rectangle);

        match input_id {
            InputId::Gamepad(input_id) => {
                self.gamepadinput2egui(input_id, state);
            }
            InputId::Keyboard(input_id) => {
                self.keyboardinput2egui(input_id, state);
            }
        };
    }

    pub fn drain_events(&mut self) -> impl Iterator<Item = Event> {
        self.events.drain(..)
    }

    fn gamepadinput2egui(&mut self, input_id: GamepadInputId, state: InputState) {
        match input_id {
            GamepadInputId::FPadUp => {}
            GamepadInputId::FPadDown => {
                self.key_event(Key::Enter, state.as_digital(None));
            }
            GamepadInputId::FPadLeft => {}
            GamepadInputId::FPadRight => {
                self.key_event(Key::Escape, state.as_digital(None));
            }
            GamepadInputId::CPadUp => {}
            GamepadInputId::CPadDown => {}
            GamepadInputId::CPadLeft => {}
            GamepadInputId::CPadRight => {}
            GamepadInputId::Select => {}
            GamepadInputId::Start => {
                self.key_event(Key::Enter, state.as_digital(None));
            }
            GamepadInputId::Mode => {}
            GamepadInputId::LeftThumb => {}
            GamepadInputId::RightThumb => {}
            GamepadInputId::DPadUp => {
                self.key_event(Key::ArrowUp, state.as_digital(None));
            }
            GamepadInputId::DPadDown => {
                self.key_event(Key::ArrowDown, state.as_digital(None));
            }
            GamepadInputId::DPadLeft => {
                self.key_event(Key::ArrowLeft, state.as_digital(None));
            }
            GamepadInputId::DPadRight => {
                self.key_event(Key::ArrowRight, state.as_digital(None));
            }
            GamepadInputId::LeftTrigger => {}
            GamepadInputId::RightTrigger => {}
            GamepadInputId::ZTrigger => {}
            GamepadInputId::LeftSecondaryTrigger => {
                self.pointer_button_event(PointerButton::Primary, state.as_digital(None));
            }
            GamepadInputId::RightSecondaryTrigger => {
                self.pointer_button_event(PointerButton::Secondary, state.as_digital(None));
            }
            GamepadInputId::LeftStickUp => {
                self.key_event(Key::ArrowUp, state.as_digital(None));
            }
            GamepadInputId::LeftStickDown => {
                self.key_event(Key::ArrowDown, state.as_digital(None));
            }
            GamepadInputId::LeftStickLeft => {
                self.key_event(Key::ArrowLeft, state.as_digital(None));
            }
            GamepadInputId::LeftStickRight => {
                self.key_event(Key::ArrowRight, state.as_digital(None));
            }
            GamepadInputId::RightStickUp => {
                self.pointer_delta_event(Vector2::new(0.0, -state.as_analog()))
            }
            GamepadInputId::RightStickDown => {
                self.pointer_delta_event(Vector2::new(0.0, state.as_analog()))
            }
            GamepadInputId::RightStickLeft => {
                self.pointer_delta_event(Vector2::new(-state.as_analog(), 0.0))
            }
            GamepadInputId::RightStickRight => {
                self.pointer_delta_event(Vector2::new(state.as_analog(), 0.0))
            }
            _ => {}
        }
    }

    #[inline]
    fn keyboardinput2egui(&mut self, input: KeyboardInputId, state: InputState) {
        static WARN_ONCE: Once = Once::new();
        WARN_ONCE.call_once(|| {
            tracing::warn!("This function should not be used, instead platform specific keyboard translation should be employed");
        });

        let key = match input {
            KeyboardInputId::Backquote => Some(Key::Backspace),
            KeyboardInputId::Backslash => Some(Key::Backslash),
            KeyboardInputId::BracketLeft => Some(Key::OpenBracket),
            KeyboardInputId::BracketRight => Some(Key::CloseBracket),
            KeyboardInputId::Comma => Some(Key::Comma),
            KeyboardInputId::Digit0 => Some(Key::Num0),
            KeyboardInputId::Digit1 => Some(Key::Num1),
            KeyboardInputId::Digit2 => Some(Key::Num2),
            KeyboardInputId::Digit3 => Some(Key::Num3),
            KeyboardInputId::Digit4 => Some(Key::Num4),
            KeyboardInputId::Digit5 => Some(Key::Num5),
            KeyboardInputId::Digit6 => Some(Key::Num6),
            KeyboardInputId::Digit7 => Some(Key::Num7),
            KeyboardInputId::Digit8 => Some(Key::Num8),
            KeyboardInputId::Digit9 => Some(Key::Num9),
            KeyboardInputId::Equal => Some(Key::Equals),
            KeyboardInputId::IntlBackslash => Some(Key::IntlBackslash),
            KeyboardInputId::IntlRo => None,
            KeyboardInputId::IntlYen => None,
            KeyboardInputId::KeyA => Some(Key::A),
            KeyboardInputId::KeyB => Some(Key::B),
            KeyboardInputId::KeyC => Some(Key::C),
            KeyboardInputId::KeyD => Some(Key::D),
            KeyboardInputId::KeyE => Some(Key::E),
            KeyboardInputId::KeyF => Some(Key::F),
            KeyboardInputId::KeyG => Some(Key::G),
            KeyboardInputId::KeyH => Some(Key::H),
            KeyboardInputId::KeyI => Some(Key::I),
            KeyboardInputId::KeyJ => Some(Key::J),
            KeyboardInputId::KeyK => Some(Key::K),
            KeyboardInputId::KeyL => Some(Key::L),
            KeyboardInputId::KeyM => Some(Key::M),
            KeyboardInputId::KeyN => Some(Key::N),
            KeyboardInputId::KeyO => Some(Key::O),
            KeyboardInputId::KeyP => Some(Key::P),
            KeyboardInputId::KeyQ => Some(Key::Q),
            KeyboardInputId::KeyR => Some(Key::R),
            KeyboardInputId::KeyS => Some(Key::S),
            KeyboardInputId::KeyT => Some(Key::T),
            KeyboardInputId::KeyU => Some(Key::U),
            KeyboardInputId::KeyV => Some(Key::V),
            KeyboardInputId::KeyW => Some(Key::W),
            KeyboardInputId::KeyX => Some(Key::X),
            KeyboardInputId::KeyY => Some(Key::Y),
            KeyboardInputId::KeyZ => Some(Key::Z),
            KeyboardInputId::Minus => Some(Key::Minus),
            KeyboardInputId::Period => Some(Key::Period),
            KeyboardInputId::Quote => Some(Key::Quote),
            KeyboardInputId::Semicolon => Some(Key::Semicolon),
            KeyboardInputId::Slash => Some(Key::Slash),
            KeyboardInputId::AltLeft => Some(Key::AltLeft),
            KeyboardInputId::AltRight => Some(Key::AltRight),
            KeyboardInputId::Backspace => Some(Key::Backspace),
            KeyboardInputId::CapsLock => None,
            KeyboardInputId::ContextMenu => None,
            KeyboardInputId::ControlLeft => Some(Key::ControlLeft),
            KeyboardInputId::ControlRight => Some(Key::ControlRight),
            KeyboardInputId::Enter => Some(Key::Enter),
            KeyboardInputId::MetaLeft => None,
            KeyboardInputId::MetaRight => None,
            KeyboardInputId::ShiftLeft => Some(Key::ShiftLeft),
            KeyboardInputId::ShiftRight => Some(Key::ShiftRight),
            KeyboardInputId::Space => Some(Key::Space),
            KeyboardInputId::Tab => Some(Key::Tab),
            KeyboardInputId::Convert => None,
            KeyboardInputId::KanaMode => None,
            KeyboardInputId::Lang1 => None,
            KeyboardInputId::Lang2 => None,
            KeyboardInputId::Lang3 => None,
            KeyboardInputId::Lang4 => None,
            KeyboardInputId::Lang5 => None,
            KeyboardInputId::NonConvert => None,
            KeyboardInputId::Delete => Some(Key::Delete),
            KeyboardInputId::End => Some(Key::End),
            KeyboardInputId::Help => None,
            KeyboardInputId::Home => Some(Key::Home),
            KeyboardInputId::Insert => Some(Key::Insert),
            KeyboardInputId::PageDown => Some(Key::PageDown),
            KeyboardInputId::PageUp => Some(Key::PageUp),
            KeyboardInputId::ArrowDown => Some(Key::ArrowDown),
            KeyboardInputId::ArrowLeft => Some(Key::ArrowLeft),
            KeyboardInputId::ArrowRight => Some(Key::ArrowRight),
            KeyboardInputId::ArrowUp => Some(Key::ArrowUp),
            KeyboardInputId::NumLock => None,
            KeyboardInputId::Numpad0 => Some(Key::Num0),
            KeyboardInputId::Numpad1 => Some(Key::Num1),
            KeyboardInputId::Numpad2 => Some(Key::Num2),
            KeyboardInputId::Numpad3 => Some(Key::Num3),
            KeyboardInputId::Numpad4 => Some(Key::Num4),
            KeyboardInputId::Numpad5 => Some(Key::Num5),
            KeyboardInputId::Numpad6 => Some(Key::Num6),
            KeyboardInputId::Numpad7 => Some(Key::Num7),
            KeyboardInputId::Numpad8 => Some(Key::Num8),
            KeyboardInputId::Numpad9 => Some(Key::Num9),
            KeyboardInputId::NumpadAdd => Some(Key::Plus),
            KeyboardInputId::NumpadBackspace => Some(Key::Backspace),
            KeyboardInputId::NumpadClear => None,
            KeyboardInputId::NumpadClearEntry => None,
            KeyboardInputId::NumpadComma => Some(Key::Comma),
            KeyboardInputId::NumpadDecimal => None,
            KeyboardInputId::NumpadDivide => None,
            KeyboardInputId::NumpadEnter => Some(Key::Enter),
            KeyboardInputId::NumpadEqual => None,
            KeyboardInputId::NumpadHash => None,
            KeyboardInputId::NumpadMemoryAdd => None,
            KeyboardInputId::NumpadMemoryClear => None,
            KeyboardInputId::NumpadMemoryRecall => None,
            KeyboardInputId::NumpadMemoryStore => None,
            KeyboardInputId::NumpadMemorySubtract => None,
            KeyboardInputId::NumpadMultiply => None,
            KeyboardInputId::NumpadParenLeft => None,
            KeyboardInputId::NumpadParenRight => None,
            KeyboardInputId::NumpadStar => None,
            KeyboardInputId::NumpadSubtract => Some(Key::Minus),
            KeyboardInputId::Escape => Some(Key::Escape),
            KeyboardInputId::Fn => None,
            KeyboardInputId::FnLock => None,
            KeyboardInputId::PrintScreen => None,
            KeyboardInputId::ScrollLock => None,
            KeyboardInputId::Pause => None,
            KeyboardInputId::BrowserBack => Some(Key::BrowserBack),
            KeyboardInputId::BrowserFavorites => None,
            KeyboardInputId::BrowserForward => None,
            KeyboardInputId::BrowserHome => None,
            KeyboardInputId::BrowserRefresh => None,
            KeyboardInputId::BrowserSearch => None,
            KeyboardInputId::BrowserStop => None,
            KeyboardInputId::Eject => None,
            KeyboardInputId::LaunchApp1 => None,
            KeyboardInputId::LaunchApp2 => None,
            KeyboardInputId::LaunchMail => None,
            KeyboardInputId::MediaPlayPause => None,
            KeyboardInputId::MediaSelect => None,
            KeyboardInputId::MediaStop => None,
            KeyboardInputId::MediaTrackNext => None,
            KeyboardInputId::MediaTrackPrevious => None,
            KeyboardInputId::Power => None,
            KeyboardInputId::Sleep => None,
            KeyboardInputId::AudioVolumeDown => None,
            KeyboardInputId::AudioVolumeMute => None,
            KeyboardInputId::AudioVolumeUp => None,
            KeyboardInputId::WakeUp => None,
            KeyboardInputId::Hyper => None,
            KeyboardInputId::SuperLeft => Some(Key::SuperLeft),
            KeyboardInputId::SuperRight => Some(Key::SuperRight),
            KeyboardInputId::Turbo => None,
            KeyboardInputId::Abort => None,
            KeyboardInputId::Resume => None,
            KeyboardInputId::Suspend => None,
            KeyboardInputId::Again => None,
            KeyboardInputId::Copy => Some(Key::Copy),
            KeyboardInputId::Cut => Some(Key::Cut),
            KeyboardInputId::Find => None,
            KeyboardInputId::Open => None,
            KeyboardInputId::Paste => Some(Key::Paste),
            KeyboardInputId::Props => None,
            KeyboardInputId::Select => None,
            KeyboardInputId::Undo => None,
            KeyboardInputId::Hiragana => None,
            KeyboardInputId::Katakana => None,
            KeyboardInputId::Unidentified => None,
            KeyboardInputId::F1 => Some(Key::F1),
            KeyboardInputId::F2 => Some(Key::F2),
            KeyboardInputId::F3 => Some(Key::F3),
            KeyboardInputId::F4 => Some(Key::F4),
            KeyboardInputId::F5 => Some(Key::F5),
            KeyboardInputId::F6 => Some(Key::F6),
            KeyboardInputId::F7 => Some(Key::F7),
            KeyboardInputId::F8 => Some(Key::F8),
            KeyboardInputId::F9 => Some(Key::F9),
            KeyboardInputId::F10 => Some(Key::F10),
            KeyboardInputId::F11 => Some(Key::F11),
            KeyboardInputId::F12 => Some(Key::F12),
            KeyboardInputId::F13 => Some(Key::F13),
            KeyboardInputId::F14 => Some(Key::F14),
            KeyboardInputId::F15 => Some(Key::F15),
            KeyboardInputId::F16 => Some(Key::F16),
            KeyboardInputId::F17 => Some(Key::F17),
            KeyboardInputId::F18 => Some(Key::F18),
            KeyboardInputId::F19 => Some(Key::F19),
            KeyboardInputId::F20 => Some(Key::F20),
            KeyboardInputId::F21 => Some(Key::F21),
            KeyboardInputId::F22 => Some(Key::F22),
            KeyboardInputId::F23 => Some(Key::F23),
            KeyboardInputId::F24 => Some(Key::F24),
            KeyboardInputId::F25 => Some(Key::F25),
            KeyboardInputId::F26 => Some(Key::F26),
            KeyboardInputId::F27 => Some(Key::F27),
            KeyboardInputId::F28 => Some(Key::F28),
            KeyboardInputId::F29 => Some(Key::F29),
            KeyboardInputId::F30 => Some(Key::F30),
            KeyboardInputId::F31 => Some(Key::F31),
            KeyboardInputId::F32 => Some(Key::F32),
            KeyboardInputId::F33 => Some(Key::F33),
            KeyboardInputId::F34 => Some(Key::F34),
            KeyboardInputId::F35 => Some(Key::F35),
            _ => None,
        };

        if let Some(key) = key {
            self.key_event(key, state.as_digital(None));
        }
    }

    fn key_event(&mut self, key: Key, pressed: bool) {
        self.events.push(Event::Key {
            key,
            physical_key: None,
            pressed,
            modifiers: Modifiers::default(),
            repeat: false,
        });
    }

    fn pointer_delta_event(&mut self, vector: Vector2<f32>) {
        let delta = vector * POINTER_SENSITIVITY;
        self.pointer_position += delta;

        if let Some(rectangle) = self.screen_rectangle {
            self.pointer_position = self
                .pointer_position
                .inf(&rectangle.max())
                .sup(&rectangle.min);
        }

        self.events.push(Event::PointerMoved(
            [self.pointer_position.x, self.pointer_position.y].into(),
        ));
    }

    fn pointer_button_event(&mut self, button: PointerButton, pressed: bool) {
        self.events.push(Event::PointerButton {
            pos: [self.pointer_position.x, self.pointer_position.y].into(),
            button,
            pressed,
            modifiers: Modifiers::default(),
        });
    }
}
