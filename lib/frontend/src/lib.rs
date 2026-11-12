pub mod audio;
mod core;
pub mod graphics;
mod library;
pub mod machine;
mod metadata;
pub mod notification;
mod platform;
mod save;
pub mod simulation_controller;
mod snapshot;
mod spawn;

pub use core::{FrontendCore, FrontendEvent, InputRouting};

pub use library::{Identification, LibraryEntry};
pub use metadata::{SlotInfo, SlotKind, SlotMetadata};
pub use platform::Platform;

rust_i18n::i18n!("locales", fallback = "en");
