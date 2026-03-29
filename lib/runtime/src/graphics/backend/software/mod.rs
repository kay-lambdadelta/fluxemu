//! # Software
//!
//! Many platforms we intend to support do not have any native graphics apis, or gpus of any kind, or are difficult to support
//!
//! This implements a meta graphic api, to provide a universal software rendering implementation

use std::{fmt::Debug, ops::BitOr};

use palette::Srgba;
pub use texture::*;

use crate::graphics::GraphicsApi;

pub mod rgb565;
mod texture;

/// Marker trait for software rendering
///
/// This is the only graphics api that is guaranteed to always work anywhere
#[derive(Default, Debug)]
pub struct Software;

/// Software backend does not and should not require any sort of extensions
///
/// Therefore this is a unit struct
#[derive(Default, Clone, Debug)]
pub struct Requirements;

impl BitOr for Requirements {
    type Output = Self;

    fn bitor(self, rhs: Self) -> Self::Output {
        rhs
    }
}

impl GraphicsApi for Software {
    /// Software backend does not and should not require any kind of initialization data
    type InitializationData = ();
    type Framebuffer = Texture<Srgba<u8>>;
    type Requirements = Requirements;
}
