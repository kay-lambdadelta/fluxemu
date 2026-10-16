//! FluxEMU Runtime
//!
//! Main runtime crate for the FluxEMU framework

pub mod component;
pub mod graphics;
mod handle;
pub mod input;
pub mod machine;
pub mod memory;
pub mod path;
pub mod platform;
pub mod scheduler;

pub use handle::RuntimeHandle;
pub(crate) use handle::{
    CURRENT_DISPATCH_TIMESTAMP, CURRENT_THREAD_RUNTIME_HANDLE, TimestampGuard,
};
pub use path::{ComponentPath, ResourcePath};
pub use platform::Platform;
