use std::{
    collections::HashMap,
    fs::{File, OpenOptions},
    os::{fd::OwnedFd, unix::fs::OpenOptionsExt},
    path::Path,
};

use egui::{
    Event, Modifiers, MouseWheelUnit, PointerButton, Pos2, TouchDeviceId, TouchId, TouchPhase,
};
use evdev::KeyCode;
use fluxemu_frontend::{Frontend, graphics::GraphicsRuntime};
use fluxemu_input::{InputId, InputState, KeyboardInputId, physical::PhysicalInputDeviceId};
use input::{
    Libinput, LibinputInterface,
    event::{
        PointerEvent, TouchEvent,
        keyboard::{KeyState, KeyboardEventTrait},
        pointer::{Axis, ButtonState, PointerScrollEvent},
        touch::{TouchEventPosition, TouchEventSlot},
    },
};
use nalgebra::Point2;
use xkbcommon::xkb::Keysym;

use crate::platform::DesktopPlatform;

const TOUCH_DEVICE: TouchDeviceId = TouchDeviceId(0);

pub struct Interface;

impl LibinputInterface for Interface {
    fn open_restricted(&mut self, path: &Path, flags: i32) -> Result<OwnedFd, i32> {
        OpenOptions::new()
            .custom_flags(flags)
            .read(true)
            .write(true)
            .open(path)
            .map(|file| file.into())
            .map_err(|err| err.raw_os_error().unwrap())
    }

    fn close_restricted(&mut self, fd: OwnedFd) {
        drop(File::from(fd));
    }
}

#[derive(Debug)]
pub struct EguiInputCollector {
    cursor_position: Option<Point2<f32>>,
    modifiers: Modifiers,
    pending_events: Vec<Event>,
    touch_positions: HashMap<u32, Point2<f32>>,
    scale_factor: f32,
}

impl EguiInputCollector {
    pub fn new(scale_factor: f32) -> Self {
        Self {
            cursor_position: None,
            modifiers: Modifiers::default(),
            pending_events: Vec::new(),
            touch_positions: HashMap::new(),
            scale_factor,
        }
    }

    pub fn take_events(&mut self) -> Vec<Event> {
        std::mem::take(&mut self.pending_events)
    }

    pub fn modifiers(&self) -> Modifiers {
        self.modifiers
    }

    pub fn handle_keyboard(
        &mut self,
        event: &input::event::KeyboardEvent,
        xkb_state: &mut xkbcommon::xkb::State,
    ) {
        // evdev -> xkb offset
        let keycode = event.key() + 8;
        let pressed = event.key_state() == KeyState::Pressed;

        xkb_state.update_key(
            xkbcommon::xkb::Keycode::new(keycode),
            if pressed {
                xkbcommon::xkb::KeyDirection::Down
            } else {
                xkbcommon::xkb::KeyDirection::Up
            },
        );

        self.modifiers = Modifiers {
            alt: xkb_state.mod_name_is_active(
                xkbcommon::xkb::MOD_NAME_ALT,
                xkbcommon::xkb::STATE_MODS_EFFECTIVE,
            ),
            ctrl: xkb_state.mod_name_is_active(
                xkbcommon::xkb::MOD_NAME_CTRL,
                xkbcommon::xkb::STATE_MODS_EFFECTIVE,
            ),
            shift: xkb_state.mod_name_is_active(
                xkbcommon::xkb::MOD_NAME_SHIFT,
                xkbcommon::xkb::STATE_MODS_EFFECTIVE,
            ),
            command: xkb_state.mod_name_is_active(
                xkbcommon::xkb::MOD_NAME_CTRL,
                xkbcommon::xkb::STATE_MODS_EFFECTIVE,
            ),
            // We are on linux
            mac_cmd: false,
        };

        let keysym = xkb_state.key_get_one_sym(xkbcommon::xkb::Keycode::new(keycode));
        if let Some(key) = keysym_to_egui_key(keysym) {
            self.pending_events.push(Event::Key {
                key,
                physical_key: None,
                pressed,
                repeat: false,
                modifiers: self.modifiers,
            });
        }

        if pressed && !self.modifiers.ctrl && !self.modifiers.alt {
            let text = xkb_state.key_get_utf8(xkbcommon::xkb::Keycode::new(keycode));

            if !text.is_empty() && text.chars().all(|c| !c.is_control()) {
                self.pending_events.push(Event::Text(text));
            }
        }
    }

    pub fn handle_pointer_motion(&mut self, event: &input::event::pointer::PointerMotionEvent) {
        let point = Point2::new(
            (self
                .cursor_position
                .map(|position| position.x)
                .unwrap_or(0.0)
                + event.dx() as f32)
                .clamp(0.0, f32::MAX),
            (self
                .cursor_position
                .map(|position| position.y)
                .unwrap_or(0.0)
                + event.dy() as f32)
                .clamp(0.0, f32::MAX),
        ) / self.scale_factor;

        self.cursor_position = Some(point);
        self.pending_events.push(Event::PointerMoved(Pos2 {
            x: point.x,
            y: point.y,
        }));
    }

    pub fn handle_pointer_button(&mut self, event: &input::event::pointer::PointerButtonEvent) {
        let Some(position) = self.cursor_position else {
            return;
        };

        let button = match KeyCode(event.button() as u16) {
            KeyCode::BTN_LEFT => PointerButton::Primary,
            KeyCode::BTN_RIGHT => PointerButton::Secondary,
            KeyCode::BTN_MIDDLE => PointerButton::Middle,
            _ => return,
        };

        let pressed = event.button_state() == ButtonState::Pressed;

        self.pending_events.push(Event::PointerButton {
            pos: Pos2 {
                x: position.x,
                y: position.y,
            },
            button,
            pressed,
            modifiers: self.modifiers,
        });
    }

    pub fn handle_pointer_motion_absolute(
        &mut self,
        event: &input::event::pointer::PointerMotionAbsoluteEvent,
        width: u16,
        height: u16,
    ) {
        let position = Point2::new(
            event.absolute_x_transformed(width as u32) as f32,
            event.absolute_y_transformed(height as u32) as f32,
        ) / self.scale_factor;

        self.cursor_position = Some(position);
        self.pending_events.push(Event::PointerMoved(Pos2 {
            x: position.x,
            y: position.y,
        }));
    }

    pub fn handle_scroll_wheel(&mut self, event: &input::event::pointer::PointerScrollWheelEvent) {
        let dx = event.scroll_value(Axis::Horizontal) as f32;
        let dy = event.scroll_value(Axis::Vertical) as f32;

        self.pending_events.push(Event::MouseWheel {
            unit: MouseWheelUnit::Point,
            delta: [dx, -dy].into(),
            modifiers: self.modifiers,
            phase: TouchPhase::Move,
        });
    }

    pub fn handle_scroll_finger(
        &mut self,
        event: &input::event::pointer::PointerScrollFingerEvent,
    ) {
        let dx = event.scroll_value(Axis::Horizontal) as f32;
        let dy = event.scroll_value(Axis::Vertical) as f32;

        self.pending_events.push(Event::MouseWheel {
            unit: MouseWheelUnit::Point,
            delta: [dx, -dy].into(),
            modifiers: self.modifiers,
            phase: TouchPhase::Move,
        });
    }

    pub fn handle_scroll_continuous(
        &mut self,
        event: &input::event::pointer::PointerScrollContinuousEvent,
    ) {
        let dx = event.scroll_value(Axis::Horizontal) as f32;
        let dy = event.scroll_value(Axis::Vertical) as f32;

        self.pending_events.push(Event::MouseWheel {
            unit: MouseWheelUnit::Point,
            delta: [dx, -dy].into(),
            modifiers: self.modifiers,
            phase: TouchPhase::Move,
        });
    }

    pub fn handle_touch_up(&mut self, event: &input::event::touch::TouchUpEvent) {
        let slot = event.seat_slot();
        let position = self.touch_positions.remove(&slot).unwrap_or_default();

        let pos = Pos2 {
            x: position.x,
            y: position.y,
        };

        self.pending_events.extend([
            Event::Touch {
                device_id: TOUCH_DEVICE,
                id: TouchId::from(slot as u64),
                phase: TouchPhase::End,
                pos,
                force: None,
            },
            Event::PointerButton {
                pos,
                button: PointerButton::Primary,
                pressed: false,
                modifiers: self.modifiers,
            },
            Event::PointerGone,
        ]);
    }

    pub fn handle_touch_down(
        &mut self,
        event: &input::event::touch::TouchDownEvent,
        width: u16,
        height: u16,
    ) {
        let position = Point2::new(
            event.x_transformed(width as u32) as f32,
            event.y_transformed(height as u32) as f32,
        ) / self.scale_factor;

        let slot = event.seat_slot();
        self.touch_positions.insert(slot, position);

        let pos = Pos2 {
            x: position.x,
            y: position.y,
        };

        self.pending_events.extend([
            Event::Touch {
                device_id: TOUCH_DEVICE,
                id: TouchId::from(slot as u64),
                phase: TouchPhase::Start,
                pos,
                force: None,
            },
            Event::PointerButton {
                pos,
                button: PointerButton::Primary,
                pressed: true,
                modifiers: self.modifiers,
            },
        ]);
    }

    pub fn handle_touch_motion(
        &mut self,
        event: &input::event::touch::TouchMotionEvent,
        width: u16,
        height: u16,
    ) {
        let position = Point2::new(
            event.x_transformed(width as u32) as f32,
            event.y_transformed(height as u32) as f32,
        ) / self.scale_factor;

        let slot = event.seat_slot();
        self.touch_positions.insert(slot, position);

        let pos = Pos2 {
            x: position.x,
            y: position.y,
        };

        self.pending_events.extend([
            Event::Touch {
                device_id: TOUCH_DEVICE,
                id: TouchId::from(slot as u64),
                phase: TouchPhase::Move,
                pos,
                force: None,
            },
            Event::PointerMoved(pos),
        ]);
    }

    pub fn handle_touch_cancel(&mut self, event: &input::event::touch::TouchCancelEvent) {
        let slot = event.seat_slot();
        let position = self.touch_positions.remove(&slot).unwrap_or_default();

        self.pending_events.extend([
            Event::Touch {
                device_id: TOUCH_DEVICE,
                id: TouchId::from(slot as u64),
                phase: TouchPhase::Cancel,
                pos: Pos2 {
                    x: position.x,
                    y: position.y,
                },
                force: None,
            },
            Event::PointerGone,
        ]);
    }
}

pub fn build_xkb_state() -> xkbcommon::xkb::State {
    let context = xkbcommon::xkb::Context::new(xkbcommon::xkb::CONTEXT_NO_FLAGS);
    let keymap = xkbcommon::xkb::Keymap::new_from_names(
        &context,
        "",
        "",
        "",
        "",
        None,
        xkbcommon::xkb::KEYMAP_COMPILE_NO_FLAGS,
    )
    .expect("failed to build xkb keymap");

    xkbcommon::xkb::State::new(&keymap)
}

pub fn handle_libinput_events<R: GraphicsRuntime>(
    frontend: &mut Frontend<DesktopPlatform<R>>,
    egui_input_collector: &mut EguiInputCollector,
    libinput: &mut Libinput,
    xkb_state: &mut xkbcommon::xkb::State,
    added_keyboard: &mut bool,
    screen_width: u16,
    screen_height: u16,
) {
    libinput.dispatch().unwrap();

    for event in libinput {
        match event {
            input::Event::Device(_device_event) => {}
            input::Event::Keyboard(keyboard_event) => {
                if !*added_keyboard {
                    *added_keyboard = true;

                    frontend.register_gamepad(
                        PhysicalInputDeviceId::PLATFORM_RESERVED,
                        "Keyboard",
                        true,
                        true,
                    );
                }

                if let Some(key) = libinput2key(evdev::KeyCode(keyboard_event.key() as u16)) {
                    frontend.insert_input(
                        PhysicalInputDeviceId::PLATFORM_RESERVED,
                        InputId::Keyboard(key),
                        match keyboard_event.key_state() {
                            KeyState::Pressed => InputState::PRESSED,
                            KeyState::Released => InputState::RELEASED,
                        },
                    );
                }

                if frontend.overlay_active() {
                    egui_input_collector.handle_keyboard(&keyboard_event, xkb_state);
                }
            }
            input::Event::Pointer(pointer_event) => {
                if !frontend.overlay_active() {
                    continue;
                }

                match pointer_event {
                    PointerEvent::Motion(motion_event) => {
                        egui_input_collector.handle_pointer_motion(&motion_event);
                    }
                    PointerEvent::MotionAbsolute(motion_event) => {
                        egui_input_collector.handle_pointer_motion_absolute(
                            &motion_event,
                            screen_width,
                            screen_height,
                        );
                    }
                    PointerEvent::Button(button_event) => {
                        egui_input_collector.handle_pointer_button(&button_event);
                    }
                    PointerEvent::ScrollWheel(scroll_event) => {
                        egui_input_collector.handle_scroll_wheel(&scroll_event);
                    }
                    PointerEvent::ScrollFinger(scroll_event) => {
                        egui_input_collector.handle_scroll_finger(&scroll_event);
                    }
                    PointerEvent::ScrollContinuous(scroll_event) => {
                        egui_input_collector.handle_scroll_continuous(&scroll_event);
                    }
                    _ => {}
                }
            }
            input::Event::Touch(touch_event) => {
                if !frontend.overlay_active() {
                    continue;
                }

                match touch_event {
                    TouchEvent::Down(down_event) => {
                        egui_input_collector.handle_touch_down(
                            &down_event,
                            screen_width,
                            screen_height,
                        );
                    }
                    TouchEvent::Motion(motion_event) => {
                        egui_input_collector.handle_touch_motion(
                            &motion_event,
                            screen_width,
                            screen_height,
                        );
                    }
                    TouchEvent::Up(up_event) => {
                        egui_input_collector.handle_touch_up(&up_event);
                    }
                    TouchEvent::Cancel(cancel_event) => {
                        egui_input_collector.handle_touch_cancel(&cancel_event);
                    }
                    _ => {}
                }
            }
            _ => {}
        }
    }
}

pub fn libinput2key(keycode: KeyCode) -> Option<KeyboardInputId> {
    match keycode {
        KeyCode::KEY_ESC => Some(KeyboardInputId::Escape),
        KeyCode::KEY_F1 => Some(KeyboardInputId::F1),
        KeyCode::KEY_F2 => Some(KeyboardInputId::F2),
        KeyCode::KEY_F3 => Some(KeyboardInputId::F3),
        KeyCode::KEY_F4 => Some(KeyboardInputId::F4),
        KeyCode::KEY_F5 => Some(KeyboardInputId::F5),
        KeyCode::KEY_F6 => Some(KeyboardInputId::F6),
        KeyCode::KEY_F7 => Some(KeyboardInputId::F7),
        KeyCode::KEY_F8 => Some(KeyboardInputId::F8),
        KeyCode::KEY_F9 => Some(KeyboardInputId::F9),
        KeyCode::KEY_F10 => Some(KeyboardInputId::F10),
        KeyCode::KEY_F11 => Some(KeyboardInputId::F11),
        KeyCode::KEY_F12 => Some(KeyboardInputId::F12),
        KeyCode::KEY_1 => Some(KeyboardInputId::Digit1),
        KeyCode::KEY_2 => Some(KeyboardInputId::Digit2),
        KeyCode::KEY_3 => Some(KeyboardInputId::Digit3),
        KeyCode::KEY_4 => Some(KeyboardInputId::Digit4),
        KeyCode::KEY_5 => Some(KeyboardInputId::Digit5),
        KeyCode::KEY_6 => Some(KeyboardInputId::Digit6),
        KeyCode::KEY_7 => Some(KeyboardInputId::Digit7),
        KeyCode::KEY_8 => Some(KeyboardInputId::Digit8),
        KeyCode::KEY_9 => Some(KeyboardInputId::Digit9),
        KeyCode::KEY_0 => Some(KeyboardInputId::Digit0),
        KeyCode::KEY_BACKSPACE => Some(KeyboardInputId::Backspace),
        KeyCode::KEY_Q => Some(KeyboardInputId::KeyQ),
        KeyCode::KEY_W => Some(KeyboardInputId::KeyW),
        KeyCode::KEY_E => Some(KeyboardInputId::KeyE),
        KeyCode::KEY_R => Some(KeyboardInputId::KeyR),
        KeyCode::KEY_T => Some(KeyboardInputId::KeyT),
        KeyCode::KEY_Y => Some(KeyboardInputId::KeyY),
        KeyCode::KEY_U => Some(KeyboardInputId::KeyU),
        KeyCode::KEY_I => Some(KeyboardInputId::KeyI),
        KeyCode::KEY_O => Some(KeyboardInputId::KeyO),
        KeyCode::KEY_P => Some(KeyboardInputId::KeyP),
        KeyCode::KEY_ENTER => Some(KeyboardInputId::Enter),
        KeyCode::KEY_A => Some(KeyboardInputId::KeyA),
        KeyCode::KEY_S => Some(KeyboardInputId::KeyS),
        KeyCode::KEY_D => Some(KeyboardInputId::KeyD),
        KeyCode::KEY_F => Some(KeyboardInputId::KeyF),
        KeyCode::KEY_G => Some(KeyboardInputId::KeyG),
        KeyCode::KEY_H => Some(KeyboardInputId::KeyH),
        KeyCode::KEY_J => Some(KeyboardInputId::KeyJ),
        KeyCode::KEY_K => Some(KeyboardInputId::KeyK),
        KeyCode::KEY_L => Some(KeyboardInputId::KeyL),
        KeyCode::KEY_Z => Some(KeyboardInputId::KeyZ),
        KeyCode::KEY_X => Some(KeyboardInputId::KeyX),
        KeyCode::KEY_C => Some(KeyboardInputId::KeyC),
        KeyCode::KEY_V => Some(KeyboardInputId::KeyV),
        KeyCode::KEY_B => Some(KeyboardInputId::KeyB),
        KeyCode::KEY_N => Some(KeyboardInputId::KeyN),
        KeyCode::KEY_M => Some(KeyboardInputId::KeyM),
        KeyCode::KEY_LEFTCTRL => Some(KeyboardInputId::ControlLeft),
        KeyCode::KEY_RIGHTCTRL => Some(KeyboardInputId::ControlRight),
        KeyCode::KEY_LEFTSHIFT => Some(KeyboardInputId::ShiftLeft),
        KeyCode::KEY_RIGHTSHIFT => Some(KeyboardInputId::ShiftRight),
        KeyCode::KEY_LEFTALT => Some(KeyboardInputId::AltLeft),
        KeyCode::KEY_RIGHTALT => Some(KeyboardInputId::AltRight),
        KeyCode::KEY_LEFTMETA => Some(KeyboardInputId::SuperLeft),
        KeyCode::KEY_RIGHTMETA => Some(KeyboardInputId::SuperRight),
        KeyCode::KEY_SPACE => Some(KeyboardInputId::Space),
        KeyCode::KEY_TAB => Some(KeyboardInputId::Tab),
        KeyCode::KEY_CAPSLOCK => Some(KeyboardInputId::CapsLock),
        KeyCode::KEY_UP => Some(KeyboardInputId::ArrowUp),
        KeyCode::KEY_DOWN => Some(KeyboardInputId::ArrowDown),
        KeyCode::KEY_LEFT => Some(KeyboardInputId::ArrowLeft),
        KeyCode::KEY_RIGHT => Some(KeyboardInputId::ArrowRight),
        KeyCode::KEY_HOME => Some(KeyboardInputId::Home),
        KeyCode::KEY_END => Some(KeyboardInputId::End),
        KeyCode::KEY_PAGEUP => Some(KeyboardInputId::PageUp),
        KeyCode::KEY_PAGEDOWN => Some(KeyboardInputId::PageDown),
        KeyCode::KEY_INSERT => Some(KeyboardInputId::Insert),
        KeyCode::KEY_DELETE => Some(KeyboardInputId::Delete),
        _ => None,
    }
}

fn keysym_to_egui_key(keysym: Keysym) -> Option<egui::Key> {
    match keysym {
        Keysym::Escape => Some(egui::Key::Escape),
        Keysym::F1 => Some(egui::Key::F1),
        Keysym::F2 => Some(egui::Key::F2),
        Keysym::F3 => Some(egui::Key::F3),
        Keysym::F4 => Some(egui::Key::F4),
        Keysym::F5 => Some(egui::Key::F5),
        Keysym::F6 => Some(egui::Key::F6),
        Keysym::F7 => Some(egui::Key::F7),
        Keysym::F8 => Some(egui::Key::F8),
        Keysym::F9 => Some(egui::Key::F9),
        Keysym::F10 => Some(egui::Key::F10),
        Keysym::F11 => Some(egui::Key::F11),
        Keysym::F12 => Some(egui::Key::F12),
        Keysym::grave | Keysym::asciitilde => Some(egui::Key::Backtick),
        Keysym::_1 | Keysym::exclam => Some(egui::Key::Num1),
        Keysym::_2 | Keysym::at => Some(egui::Key::Num2),
        Keysym::_3 | Keysym::numbersign => Some(egui::Key::Num3),
        Keysym::_4 | Keysym::dollar => Some(egui::Key::Num4),
        Keysym::_5 | Keysym::percent => Some(egui::Key::Num5),
        Keysym::_6 | Keysym::asciicircum => Some(egui::Key::Num6),
        Keysym::_7 | Keysym::ampersand => Some(egui::Key::Num7),
        Keysym::_8 | Keysym::asterisk => Some(egui::Key::Num8),
        Keysym::_9 | Keysym::parenleft => Some(egui::Key::Num9),
        Keysym::_0 | Keysym::parenright => Some(egui::Key::Num0),
        Keysym::minus | Keysym::underscore => Some(egui::Key::Minus),
        Keysym::equal | Keysym::plus => Some(egui::Key::Equals),
        Keysym::BackSpace => Some(egui::Key::Backspace),
        Keysym::Q | Keysym::q => Some(egui::Key::Q),
        Keysym::W | Keysym::w => Some(egui::Key::W),
        Keysym::E | Keysym::e => Some(egui::Key::E),
        Keysym::R | Keysym::r => Some(egui::Key::R),
        Keysym::T | Keysym::t => Some(egui::Key::T),
        Keysym::Y | Keysym::y => Some(egui::Key::Y),
        Keysym::U | Keysym::u => Some(egui::Key::U),
        Keysym::I | Keysym::i => Some(egui::Key::I),
        Keysym::O | Keysym::o => Some(egui::Key::O),
        Keysym::P | Keysym::p => Some(egui::Key::P),
        Keysym::bracketleft | Keysym::braceleft => Some(egui::Key::OpenBracket),
        Keysym::bracketright | Keysym::braceright => Some(egui::Key::CloseBracket),
        Keysym::backslash | Keysym::bar => Some(egui::Key::Backslash),
        Keysym::Return => Some(egui::Key::Enter),
        Keysym::A | Keysym::a => Some(egui::Key::A),
        Keysym::S | Keysym::s => Some(egui::Key::S),
        Keysym::D | Keysym::d => Some(egui::Key::D),
        Keysym::F | Keysym::f => Some(egui::Key::F),
        Keysym::G | Keysym::g => Some(egui::Key::G),
        Keysym::H | Keysym::h => Some(egui::Key::H),
        Keysym::J | Keysym::j => Some(egui::Key::J),
        Keysym::K | Keysym::k => Some(egui::Key::K),
        Keysym::L | Keysym::l => Some(egui::Key::L),
        Keysym::semicolon | Keysym::colon => Some(egui::Key::Semicolon),
        Keysym::apostrophe | Keysym::quotedbl => Some(egui::Key::Quote),
        Keysym::Z | Keysym::z => Some(egui::Key::Z),
        Keysym::X | Keysym::x => Some(egui::Key::X),
        Keysym::C | Keysym::c => Some(egui::Key::C),
        Keysym::V | Keysym::v => Some(egui::Key::V),
        Keysym::B | Keysym::b => Some(egui::Key::B),
        Keysym::N | Keysym::n => Some(egui::Key::N),
        Keysym::M | Keysym::m => Some(egui::Key::M),
        Keysym::comma | Keysym::less => Some(egui::Key::Comma),
        Keysym::period | Keysym::greater => Some(egui::Key::Period),
        Keysym::slash | Keysym::question => Some(egui::Key::Slash),
        Keysym::space => Some(egui::Key::Space),
        Keysym::Tab => Some(egui::Key::Tab),
        Keysym::Up => Some(egui::Key::ArrowUp),
        Keysym::Down => Some(egui::Key::ArrowDown),
        Keysym::Left => Some(egui::Key::ArrowLeft),
        Keysym::Right => Some(egui::Key::ArrowRight),
        Keysym::Home => Some(egui::Key::Home),
        Keysym::End => Some(egui::Key::End),
        Keysym::Page_Up => Some(egui::Key::PageUp),
        Keysym::Page_Down => Some(egui::Key::PageDown),
        Keysym::Insert => Some(egui::Key::Insert),
        Keysym::Delete => Some(egui::Key::Delete),
        _ => None,
    }
}
