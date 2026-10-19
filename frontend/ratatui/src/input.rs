use std::{
    collections::HashMap,
    time::{Duration, Instant},
};

use crossterm::event::{KeyCode, MediaKeyCode, ModifierKeyCode};
use fluxemu_input::{GamepadInputId, KeyboardInputId};

const SYNTHETIC_RELEASE_DELAY: Duration = Duration::from_millis(200);

pub fn crossterm2key(code: KeyCode) -> Option<KeyboardInputId> {
    Some(match code {
        KeyCode::Char(character) => char2key(character)?,
        KeyCode::Backspace => KeyboardInputId::Backspace,
        KeyCode::Enter => KeyboardInputId::Enter,
        KeyCode::Left => KeyboardInputId::ArrowLeft,
        KeyCode::Right => KeyboardInputId::ArrowRight,
        KeyCode::Up => KeyboardInputId::ArrowUp,
        KeyCode::Down => KeyboardInputId::ArrowDown,
        KeyCode::Home => KeyboardInputId::Home,
        KeyCode::End => KeyboardInputId::End,
        KeyCode::PageUp => KeyboardInputId::PageUp,
        KeyCode::PageDown => KeyboardInputId::PageDown,
        KeyCode::Tab | KeyCode::BackTab => KeyboardInputId::Tab,
        KeyCode::Delete => KeyboardInputId::Delete,
        KeyCode::Insert => KeyboardInputId::Insert,
        KeyCode::F(number) => function2key(number)?,
        KeyCode::Esc => KeyboardInputId::Escape,
        KeyCode::CapsLock => KeyboardInputId::CapsLock,
        KeyCode::ScrollLock => KeyboardInputId::ScrollLock,
        KeyCode::NumLock => KeyboardInputId::NumLock,
        KeyCode::PrintScreen => KeyboardInputId::PrintScreen,
        KeyCode::Pause => KeyboardInputId::Pause,
        KeyCode::Menu => KeyboardInputId::ContextMenu,
        KeyCode::Media(media) => match media {
            MediaKeyCode::PlayPause | MediaKeyCode::Play | MediaKeyCode::Pause => {
                KeyboardInputId::MediaPlayPause
            }
            MediaKeyCode::Stop => KeyboardInputId::MediaStop,
            MediaKeyCode::TrackNext => KeyboardInputId::MediaTrackNext,
            MediaKeyCode::TrackPrevious => KeyboardInputId::MediaTrackPrevious,
            MediaKeyCode::LowerVolume => KeyboardInputId::AudioVolumeDown,
            MediaKeyCode::RaiseVolume => KeyboardInputId::AudioVolumeUp,
            MediaKeyCode::MuteVolume => KeyboardInputId::AudioVolumeMute,
            _ => return None,
        },
        KeyCode::Modifier(modifier) => match modifier {
            ModifierKeyCode::LeftShift => KeyboardInputId::ShiftLeft,
            ModifierKeyCode::RightShift => KeyboardInputId::ShiftRight,
            ModifierKeyCode::LeftControl => KeyboardInputId::ControlLeft,
            ModifierKeyCode::RightControl => KeyboardInputId::ControlRight,
            ModifierKeyCode::LeftAlt => KeyboardInputId::AltLeft,
            ModifierKeyCode::RightAlt => KeyboardInputId::AltRight,
            ModifierKeyCode::LeftSuper => KeyboardInputId::SuperLeft,
            ModifierKeyCode::RightSuper => KeyboardInputId::SuperRight,
            ModifierKeyCode::LeftMeta => KeyboardInputId::MetaLeft,
            ModifierKeyCode::LeftHyper | ModifierKeyCode::RightHyper => KeyboardInputId::Hyper,
            _ => return None,
        },
        _ => return None,
    })
}

fn char2key(character: char) -> Option<KeyboardInputId> {
    Some(match character.to_ascii_lowercase() {
        'a' => KeyboardInputId::KeyA,
        'b' => KeyboardInputId::KeyB,
        'c' => KeyboardInputId::KeyC,
        'd' => KeyboardInputId::KeyD,
        'e' => KeyboardInputId::KeyE,
        'f' => KeyboardInputId::KeyF,
        'g' => KeyboardInputId::KeyG,
        'h' => KeyboardInputId::KeyH,
        'i' => KeyboardInputId::KeyI,
        'j' => KeyboardInputId::KeyJ,
        'k' => KeyboardInputId::KeyK,
        'l' => KeyboardInputId::KeyL,
        'm' => KeyboardInputId::KeyM,
        'n' => KeyboardInputId::KeyN,
        'o' => KeyboardInputId::KeyO,
        'p' => KeyboardInputId::KeyP,
        'q' => KeyboardInputId::KeyQ,
        'r' => KeyboardInputId::KeyR,
        's' => KeyboardInputId::KeyS,
        't' => KeyboardInputId::KeyT,
        'u' => KeyboardInputId::KeyU,
        'v' => KeyboardInputId::KeyV,
        'w' => KeyboardInputId::KeyW,
        'x' => KeyboardInputId::KeyX,
        'y' => KeyboardInputId::KeyY,
        'z' => KeyboardInputId::KeyZ,
        '0' | ')' => KeyboardInputId::Digit0,
        '1' | '!' => KeyboardInputId::Digit1,
        '2' | '@' => KeyboardInputId::Digit2,
        '3' | '#' => KeyboardInputId::Digit3,
        '4' | '$' => KeyboardInputId::Digit4,
        '5' | '%' => KeyboardInputId::Digit5,
        '6' | '^' => KeyboardInputId::Digit6,
        '7' | '&' => KeyboardInputId::Digit7,
        '8' | '*' => KeyboardInputId::Digit8,
        '9' | '(' => KeyboardInputId::Digit9,
        ' ' => KeyboardInputId::Space,
        '`' | '~' => KeyboardInputId::Backquote,
        '-' | '_' => KeyboardInputId::Minus,
        '=' | '+' => KeyboardInputId::Equal,
        '[' | '{' => KeyboardInputId::BracketLeft,
        ']' | '}' => KeyboardInputId::BracketRight,
        '\\' | '|' => KeyboardInputId::Backslash,
        ';' | ':' => KeyboardInputId::Semicolon,
        '\'' | '"' => KeyboardInputId::Quote,
        ',' | '<' => KeyboardInputId::Comma,
        '.' | '>' => KeyboardInputId::Period,
        '/' | '?' => KeyboardInputId::Slash,
        _ => return None,
    })
}

fn function2key(number: u8) -> Option<KeyboardInputId> {
    Some(match number {
        1 => KeyboardInputId::F1,
        2 => KeyboardInputId::F2,
        3 => KeyboardInputId::F3,
        4 => KeyboardInputId::F4,
        5 => KeyboardInputId::F5,
        6 => KeyboardInputId::F6,
        7 => KeyboardInputId::F7,
        8 => KeyboardInputId::F8,
        9 => KeyboardInputId::F9,
        10 => KeyboardInputId::F10,
        11 => KeyboardInputId::F11,
        12 => KeyboardInputId::F12,
        13 => KeyboardInputId::F13,
        14 => KeyboardInputId::F14,
        15 => KeyboardInputId::F15,
        16 => KeyboardInputId::F16,
        17 => KeyboardInputId::F17,
        18 => KeyboardInputId::F18,
        19 => KeyboardInputId::F19,
        20 => KeyboardInputId::F20,
        21 => KeyboardInputId::F21,
        22 => KeyboardInputId::F22,
        23 => KeyboardInputId::F23,
        24 => KeyboardInputId::F24,
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
    last_pressed: HashMap<KeyboardInputId, Instant>,
}

impl SyntheticReleaseTracker {
    pub fn press(&mut self, key: KeyboardInputId) {
        self.last_pressed.insert(key, Instant::now());
    }

    pub fn release(&mut self, key: KeyboardInputId) {
        self.last_pressed.remove(&key);
    }

    pub fn drain_expired(&mut self) -> Vec<KeyboardInputId> {
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
