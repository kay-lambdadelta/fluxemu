use std::{fmt::Debug, ops::BitOr};

use palette::Srgba;
pub use texture::*;

use crate::graphics::GraphicsApi;

mod rgb565;
mod texture;

/// Marker trait for software rendering
///
/// This is the only graphics api that is guaranteed to always work anywhere
#[derive(Default, Debug)]
pub struct Software;

#[derive(Default, Clone, Debug)]
/// Does not actually require any extensions
pub struct Requirements;

impl BitOr for Requirements {
    type Output = Self;

    fn bitor(self, rhs: Self) -> Self::Output {
        rhs
    }
}

impl GraphicsApi for Software {
    type InitializationData = ();
    type Framebuffer = Texture<Srgba<u8>>;
    type Requirements = Requirements;
}
