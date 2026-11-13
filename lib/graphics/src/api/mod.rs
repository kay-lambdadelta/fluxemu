use core::{any::Any, fmt::Debug, ops::BitOr};

use nalgebra::Vector2;
use palette::Srgba;

use crate::texture::RefTexture;

/// Software
pub mod software;

/// Webgpu
#[cfg(feature = "webgpu")]
pub mod webgpu;

/// Trait for marker structs representing rendering backends
pub trait GraphicsApi: Debug + Any + Sized + Send + Sync + 'static {
    const NAME: &'static str;

    /// State graphics jobs run against, owned by the thread that owns the graphics api
    type Context: 'static;
    /// The component framebuffer type
    type Framebuffer: Framebuffer;
    /// How components describe what they require out of a graphics context
    type Requirements: Default
        + BitOr<Output = Self::Requirements>
        + Clone
        + Debug
        + Send
        + Sync
        + 'static;

    fn create_software_framebuffer(
        context: &mut Self::Context,
        size: Vector2<usize>,
    ) -> Self::Framebuffer;

    fn upload_software_frame(
        context: &mut Self::Context,
        framebuffer: &mut Self::Framebuffer,
        frame: RefTexture<'_, Srgba<u8>>,
    );
}

pub trait Framebuffer: Any + Debug + 'static {
    fn width(&self) -> usize;
    fn height(&self) -> usize;
}
