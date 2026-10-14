pub mod audio;
mod core;
pub mod graphics;
mod input;
pub mod machine;
pub mod notification;
mod platform;
pub mod simulation_controller;

pub use core::{FrontendCore, FrontendEvent, InputRouting};

pub use platform::Platform;

rust_i18n::i18n!("locales", fallback = "en");
