use std::{collections::HashSet, sync::Once};

use egui::{Context, Event, Key, Modifiers, PointerButton, ViewportId};
use fluxemu_input::{GamepadInputId, InputId, InputState};
use fluxemu_math::rectangle::Rectangle;
use keyboard_types::Code;
use nalgebra::{Point2, Vector2};

const POINTER_SENSITIVITY: f32 = 2.0;

pub struct EguiInputTranslator {
    events: Vec<Event>,
    held_keys: HashSet<Key>,
    held_pointer_buttons: Vec<PointerButton>,
    pointer_position: Point2<f32>,
    screen_rectangle: Option<Rectangle<f32>>,
}

impl Default for EguiInputTranslator {
    fn default() -> Self {
        Self {
            events: Vec::new(),
            held_keys: HashSet::new(),
            held_pointer_buttons: Vec::new(),
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

    pub fn release_all(&mut self) {
        for key in std::mem::take(&mut self.held_keys) {
            self.key_event(key, false);
        }

        for button in std::mem::take(&mut self.held_pointer_buttons) {
            self.pointer_button_event(button, false);
        }
    }

    fn gamepadinput2egui(&mut self, input_id: GamepadInputId, state: InputState) {
        match input_id {
            GamepadInputId::FPadUp => {
                self.key_event(Key::Menu, state.as_digital(None));
            }
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
    fn keyboardinput2egui(&mut self, input: Code, state: InputState) {
        static WARN_ONCE: Once = Once::new();
        WARN_ONCE.call_once(|| {
            tracing::warn!(
                "This function should not be used, instead platform specific keyboard translation \
                 should be employed"
            );
        });

        let key = match input {
            Code::Backquote => Some(Key::Backtick),
            Code::Backslash => Some(Key::Backslash),
            Code::BracketLeft => Some(Key::OpenBracket),
            Code::BracketRight => Some(Key::CloseBracket),
            Code::Comma => Some(Key::Comma),
            Code::Digit0 => Some(Key::Num0),
            Code::Digit1 => Some(Key::Num1),
            Code::Digit2 => Some(Key::Num2),
            Code::Digit3 => Some(Key::Num3),
            Code::Digit4 => Some(Key::Num4),
            Code::Digit5 => Some(Key::Num5),
            Code::Digit6 => Some(Key::Num6),
            Code::Digit7 => Some(Key::Num7),
            Code::Digit8 => Some(Key::Num8),
            Code::Digit9 => Some(Key::Num9),
            Code::Equal => Some(Key::Equals),
            Code::IntlBackslash => Some(Key::IntlBackslash),
            Code::KeyA => Some(Key::A),
            Code::KeyB => Some(Key::B),
            Code::KeyC => Some(Key::C),
            Code::KeyD => Some(Key::D),
            Code::KeyE => Some(Key::E),
            Code::KeyF => Some(Key::F),
            Code::KeyG => Some(Key::G),
            Code::KeyH => Some(Key::H),
            Code::KeyI => Some(Key::I),
            Code::KeyJ => Some(Key::J),
            Code::KeyK => Some(Key::K),
            Code::KeyL => Some(Key::L),
            Code::KeyM => Some(Key::M),
            Code::KeyN => Some(Key::N),
            Code::KeyO => Some(Key::O),
            Code::KeyP => Some(Key::P),
            Code::KeyQ => Some(Key::Q),
            Code::KeyR => Some(Key::R),
            Code::KeyS => Some(Key::S),
            Code::KeyT => Some(Key::T),
            Code::KeyU => Some(Key::U),
            Code::KeyV => Some(Key::V),
            Code::KeyW => Some(Key::W),
            Code::KeyX => Some(Key::X),
            Code::KeyY => Some(Key::Y),
            Code::KeyZ => Some(Key::Z),
            Code::Minus => Some(Key::Minus),
            Code::Period => Some(Key::Period),
            Code::Quote => Some(Key::Quote),
            Code::Semicolon => Some(Key::Semicolon),
            Code::Slash => Some(Key::Slash),
            Code::AltLeft => Some(Key::AltLeft),
            Code::AltRight => Some(Key::AltRight),
            Code::Backspace => Some(Key::Backspace),
            Code::ContextMenu => Some(Key::Menu),
            Code::ControlLeft => Some(Key::ControlLeft),
            Code::ControlRight => Some(Key::ControlRight),
            Code::Enter => Some(Key::Enter),
            Code::MetaLeft => Some(Key::SuperLeft),
            Code::MetaRight => Some(Key::SuperRight),
            Code::ShiftLeft => Some(Key::ShiftLeft),
            Code::ShiftRight => Some(Key::ShiftRight),
            Code::Space => Some(Key::Space),
            Code::Tab => Some(Key::Tab),
            Code::Delete => Some(Key::Delete),
            Code::End => Some(Key::End),
            Code::Home => Some(Key::Home),
            Code::Insert => Some(Key::Insert),
            Code::PageDown => Some(Key::PageDown),
            Code::PageUp => Some(Key::PageUp),
            Code::ArrowDown => Some(Key::ArrowDown),
            Code::ArrowLeft => Some(Key::ArrowLeft),
            Code::ArrowRight => Some(Key::ArrowRight),
            Code::ArrowUp => Some(Key::ArrowUp),
            Code::Numpad0 => Some(Key::Num0),
            Code::Numpad1 => Some(Key::Num1),
            Code::Numpad2 => Some(Key::Num2),
            Code::Numpad3 => Some(Key::Num3),
            Code::Numpad4 => Some(Key::Num4),
            Code::Numpad5 => Some(Key::Num5),
            Code::Numpad6 => Some(Key::Num6),
            Code::Numpad7 => Some(Key::Num7),
            Code::Numpad8 => Some(Key::Num8),
            Code::Numpad9 => Some(Key::Num9),
            Code::NumpadAdd => Some(Key::Plus),
            Code::NumpadBackspace => Some(Key::Backspace),
            Code::NumpadComma => Some(Key::Comma),
            Code::NumpadEnter => Some(Key::Enter),
            Code::NumpadSubtract => Some(Key::Minus),
            Code::Escape => Some(Key::Escape),
            Code::BrowserBack => Some(Key::BrowserBack),
            Code::Copy => Some(Key::Copy),
            Code::Cut => Some(Key::Cut),
            Code::Paste => Some(Key::Paste),
            Code::F1 => Some(Key::F1),
            Code::F2 => Some(Key::F2),
            Code::F3 => Some(Key::F3),
            Code::F4 => Some(Key::F4),
            Code::F5 => Some(Key::F5),
            Code::F6 => Some(Key::F6),
            Code::F7 => Some(Key::F7),
            Code::F8 => Some(Key::F8),
            Code::F9 => Some(Key::F9),
            Code::F10 => Some(Key::F10),
            Code::F11 => Some(Key::F11),
            Code::F12 => Some(Key::F12),
            Code::F13 => Some(Key::F13),
            Code::F14 => Some(Key::F14),
            Code::F15 => Some(Key::F15),
            Code::F16 => Some(Key::F16),
            Code::F17 => Some(Key::F17),
            Code::F18 => Some(Key::F18),
            Code::F19 => Some(Key::F19),
            Code::F20 => Some(Key::F20),
            Code::F21 => Some(Key::F21),
            Code::F22 => Some(Key::F22),
            Code::F23 => Some(Key::F23),
            Code::F24 => Some(Key::F24),
            Code::F25 => Some(Key::F25),
            Code::F26 => Some(Key::F26),
            Code::F27 => Some(Key::F27),
            Code::F28 => Some(Key::F28),
            Code::F29 => Some(Key::F29),
            Code::F30 => Some(Key::F30),
            Code::F31 => Some(Key::F31),
            Code::F32 => Some(Key::F32),
            Code::F33 => Some(Key::F33),
            Code::F34 => Some(Key::F34),
            Code::F35 => Some(Key::F35),
            _ => None,
        };

        if let Some(key) = key {
            self.key_event(key, state.as_digital(None));
        }
    }

    fn key_event(&mut self, key: Key, pressed: bool) {
        let changed = if pressed {
            self.held_keys.insert(key)
        } else {
            self.held_keys.remove(&key)
        };

        if !changed {
            return;
        }

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
                .inf(&rectangle.max)
                .sup(&rectangle.min);
        }

        self.events.push(Event::PointerMoved(
            [self.pointer_position.x, self.pointer_position.y].into(),
        ));
    }

    fn pointer_button_event(&mut self, button: PointerButton, pressed: bool) {
        let was_pressed = self.held_pointer_buttons.contains(&button);

        if was_pressed == pressed {
            return;
        }

        if pressed {
            self.held_pointer_buttons.push(button);
        } else {
            self.held_pointer_buttons.retain(|held| *held != button);
        }

        self.events.push(Event::PointerButton {
            pos: [self.pointer_position.x, self.pointer_position.y].into(),
            button,
            pressed,
            modifiers: Modifiers::default(),
        });
    }
}
