//! FluxEMU Runtime
//!
//! Main runtime crate for the FluxEMU framework

#![allow(async_fn_in_trait)]

/// Component facing runtime api
mod api;
/// Basic types relating to the fundemental unit of this emulator
pub mod component;
/// Graphics definitions
pub mod graphics;
/// Input definitions
pub mod input;
/// Machine builder and definition
pub mod machine;
/// Memory access utilities
pub mod memory;
/// Path
pub mod path;
/// Saves and snapshots
pub mod persistence;
/// Platform description utilities
pub mod platform;
/// Emulated processor utilities
pub mod processor;
/// Emulator scheduler
pub mod scheduler;

pub use api::*;
