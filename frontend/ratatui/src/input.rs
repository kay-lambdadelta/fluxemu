use std::{
    collections::HashMap,
    time::{Duration, Instant},
};

use crossterm::event::{KeyCode, MediaKeyCode, ModifierKeyCode};
use fluxemu_input::GamepadInputId;
use keyboard_types::Code;

const SYNTHETIC_RELEASE_DELAY: Duration = Duration::from_millis(200);

pub fn crossterm2key(code: KeyCode) -> Option<Code> {
    Some(match code {
        KeyCode::Char(character) => char2key(character)?,
        KeyCode::Backspace => Code::Backspace,
        KeyCode::Enter => Code::Enter,
        KeyCode::Left => Code::ArrowLeft,
        KeyCode::Right => Code::ArrowRight,
        KeyCode::Up => Code::ArrowUp,
        KeyCode::Down => Code::ArrowDown,
        KeyCode::Home => Code::Home,
        KeyCode::End => Code::End,
        KeyCode::PageUp => Code::PageUp,
        KeyCode::PageDown => Code::PageDown,
        KeyCode::Tab | KeyCode::BackTab => Code::Tab,
        KeyCode::Delete => Code::Delete,
        KeyCode::Insert => Code::Insert,
        KeyCode::F(number) => function2key(number)?,
        KeyCode::Esc => Code::Escape,
        KeyCode::CapsLock => Code::CapsLock,
        KeyCode::ScrollLock => Code::ScrollLock,
        KeyCode::NumLock => Code::NumLock,
        KeyCode::PrintScreen => Code::PrintScreen,
        KeyCode::Pause => Code::Pause,
        KeyCode::Menu => Code::ContextMenu,
        KeyCode::Media(media) => match media {
            MediaKeyCode::PlayPause | MediaKeyCode::Play | MediaKeyCode::Pause => {
                Code::MediaPlayPause
            }
            MediaKeyCode::Stop => Code::MediaStop,
            MediaKeyCode::TrackNext => Code::MediaTrackNext,
            MediaKeyCode::TrackPrevious => Code::MediaTrackPrevious,
            MediaKeyCode::LowerVolume => Code::AudioVolumeDown,
            MediaKeyCode::RaiseVolume => Code::AudioVolumeUp,
            MediaKeyCode::MuteVolume => Code::AudioVolumeMute,
            _ => return None,
        },
        KeyCode::Modifier(modifier) => match modifier {
            ModifierKeyCode::LeftShift => Code::ShiftLeft,
            ModifierKeyCode::RightShift => Code::ShiftRight,
            ModifierKeyCode::LeftControl => Code::ControlLeft,
            ModifierKeyCode::RightControl => Code::ControlRight,
            ModifierKeyCode::LeftAlt => Code::AltLeft,
            ModifierKeyCode::RightAlt => Code::AltRight,
            ModifierKeyCode::LeftSuper => Code::MetaLeft,
            ModifierKeyCode::RightSuper => Code::MetaRight,
            _ => return None,
        },
        _ => return None,
    })
}

fn char2key(character: char) -> Option<Code> {
    Some(match character.to_ascii_lowercase() {
        'a' => Code::KeyA,
        'b' => Code::KeyB,
        'c' => Code::KeyC,
        'd' => Code::KeyD,
        'e' => Code::KeyE,
        'f' => Code::KeyF,
        'g' => Code::KeyG,
        'h' => Code::KeyH,
        'i' => Code::KeyI,
        'j' => Code::KeyJ,
        'k' => Code::KeyK,
        'l' => Code::KeyL,
        'm' => Code::KeyM,
        'n' => Code::KeyN,
        'o' => Code::KeyO,
        'p' => Code::KeyP,
        'q' => Code::KeyQ,
        'r' => Code::KeyR,
        's' => Code::KeyS,
        't' => Code::KeyT,
        'u' => Code::KeyU,
        'v' => Code::KeyV,
        'w' => Code::KeyW,
        'x' => Code::KeyX,
        'y' => Code::KeyY,
        'z' => Code::KeyZ,
        '0' | ')' => Code::Digit0,
        '1' | '!' => Code::Digit1,
        '2' | '@' => Code::Digit2,
        '3' | '#' => Code::Digit3,
        '4' | '$' => Code::Digit4,
        '5' | '%' => Code::Digit5,
        '6' | '^' => Code::Digit6,
        '7' | '&' => Code::Digit7,
        '8' | '*' => Code::Digit8,
        '9' | '(' => Code::Digit9,
        ' ' => Code::Space,
        '`' | '~' => Code::Backquote,
        '-' | '_' => Code::Minus,
        '=' | '+' => Code::Equal,
        '[' | '{' => Code::BracketLeft,
        ']' | '}' => Code::BracketRight,
        '\\' | '|' => Code::Backslash,
        ';' | ':' => Code::Semicolon,
        '\'' | '"' => Code::Quote,
        ',' | '<' => Code::Comma,
        '.' | '>' => Code::Period,
        '/' | '?' => Code::Slash,
        _ => return None,
    })
}

fn function2key(number: u8) -> Option<Code> {
    Some(match number {
        1 => Code::F1,
        2 => Code::F2,
        3 => Code::F3,
        4 => Code::F4,
        5 => Code::F5,
        6 => Code::F6,
        7 => Code::F7,
        8 => Code::F8,
        9 => Code::F9,
        10 => Code::F10,
        11 => Code::F11,
        12 => Code::F12,
        13 => Code::F13,
        14 => Code::F14,
        15 => Code::F15,
        16 => Code::F16,
        17 => Code::F17,
        18 => Code::F18,
        19 => Code::F19,
        20 => Code::F20,
        21 => Code::F21,
        22 => Code::F22,
        23 => Code::F23,
        24 => Code::F24,
        _ => return None,
    })
}

pub fn gamepad2key(input_id: GamepadInputId) -> Option<KeyCode> {
    Some(match input_id {
        GamepadInputId::DPadUp | GamepadInputId::LeftStickUp => KeyCode::Up,
        GamepadInputId::DPadDown | GamepadInputId::LeftStickDown => KeyCode::Down,
        GamepadInputId::DPadLeft | GamepadInputId::LeftStickLeft => KeyCode::Left,
        GamepadInputId::DPadRight | GamepadInputId::LeftStickRight => KeyCode::Right,
        GamepadInputId::FPadDown => KeyCode::Enter,
        GamepadInputId::FPadRight => KeyCode::Backspace,
        GamepadInputId::LeftTrigger => KeyCode::BackTab,
        GamepadInputId::RightTrigger => KeyCode::Tab,
        GamepadInputId::Start => KeyCode::Esc,
        _ => return None,
    })
}

#[derive(Debug, Default)]
pub struct SyntheticReleaseTracker {
    last_pressed: HashMap<Code, Instant>,
}

impl SyntheticReleaseTracker {
    pub fn press(&mut self, key: Code) {
        self.last_pressed.insert(key, Instant::now());
    }

    pub fn release(&mut self, key: Code) {
        self.last_pressed.remove(&key);
    }

    pub fn drain_expired(&mut self) -> Vec<Code> {
        let now = Instant::now();
        let mut expired = Vec::new();

        self.last_pressed.retain(|key, pressed_at| {
            let is_held = now.duration_since(*pressed_at) < SYNTHETIC_RELEASE_DELAY;

            if !is_held {
                expired.push(*key);
            }

            is_held
        });

        expired
    }

    pub fn next_deadline(&self) -> Option<Instant> {
        self.last_pressed
            .values()
            .min()
            .map(|pressed_at| *pressed_at + SYNTHETIC_RELEASE_DELAY)
    }
}
