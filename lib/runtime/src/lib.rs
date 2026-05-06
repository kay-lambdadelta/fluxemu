//! FluxEMU Runtime
//!
//! Main runtime crate for the FluxEMU framework

#![cfg_attr(not(feature = "std"), no_std)]

pub mod component;
pub mod event;
pub mod graphics;
mod handle;
pub mod input;
pub mod machine;
pub mod memory;
pub mod path;
pub mod persistence;
pub mod platform;
pub mod scheduler;

pub use handle::*;
pub use path::{ComponentPath, ResourcePath};
pub use platform::Platform;
