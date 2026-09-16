pub mod audio;
pub mod graphics;
mod input;
pub mod machine;
mod platform;
pub mod simulation_controller;

pub use platform::Platform;

rust_i18n::i18n!("locales", fallback = "en");
