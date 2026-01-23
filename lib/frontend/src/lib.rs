//! Generic multi platform frontend implementation for fluxemu

#![deny(missing_docs)]

mod backend;

/// Various configurations this frontend consumes
pub mod environment;
mod frontend;
mod gui;
mod hotkey;
mod machine_factories;
mod platform;

pub use backend::*;
pub use frontend::*;
pub use gui::software_rendering as gui_software_rendering;
pub use hotkey::*;
pub use machine_factories::MachineFactories;
pub use platform::*;

/// Canonical shader for egui rendering
///
/// TODO: Test if converting this with naga is actually suitable for opengl
pub const EGUI_WGSL_SHADER: &str = include_str!("egui.wgsl");
