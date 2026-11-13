//! # Software
//!
//! Many platforms we intend to support do not have any native graphics apis, or gpus of any kind, or are difficult to support
//!
//! This implements a meta graphic api, to provide a universal software rendering implementation

use core::{fmt::Debug, ops::BitOr};

use nalgebra::Vector2;
use palette::{Srgba, named::BLACK};

use crate::{
    api::{Framebuffer, GraphicsApi},
    texture::{CopyMode, OwnedTexture, RefTexture},
};

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

impl Framebuffer for OwnedTexture<Srgba<u8>> {
    fn width(&self) -> usize {
        self.width()
    }

    fn height(&self) -> usize {
        self.height()
    }
}

impl GraphicsApi for Software {
    /// Software backend does not and should not require any kind of context
    type Context = ();
    type Framebuffer = OwnedTexture<Srgba<u8>>;
    type Requirements = Requirements;

    const NAME: &'static str = "Software";

    fn create_software_framebuffer(_context: &mut (), size: Vector2<usize>) -> Self::Framebuffer {
        OwnedTexture::from_value(size.x, size.y, BLACK.into())
    }

    fn upload_software_frame(
        _context: &mut (),
        framebuffer: &mut Self::Framebuffer,
        frame: RefTexture<'_, Srgba<u8>>,
    ) {
        framebuffer.copy_from(frame, CopyMode::Nearest);
    }
}
