use std::{any::Any, fmt::Debug, ops::BitOr};

/// Software
pub mod software;

/// Webgpu
#[cfg(feature = "webgpu")]
pub mod webgpu;

/// Trait for marker structs representing rendering backends
pub trait GraphicsApi: Debug + Any + Sized + Send + Sync + 'static {
    /// Data components need to do their graphics operations
    type InitializationData: Clone + Debug + 'static;
    /// The component framebuffer type
    type Framebuffer: Any + Send + Sync + Debug + 'static;
    /// How components describe what they require out of a graphics context
    type Requirements: Default
        + BitOr<Output = Self::Requirements>
        + Clone
        + Debug
        + Send
        + Sync
        + 'static;
}
