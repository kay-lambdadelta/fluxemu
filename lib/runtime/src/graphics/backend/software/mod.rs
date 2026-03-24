use std::{fmt::Debug, ops::BitOr};

use palette::Srgba;
pub use texture::*;

use crate::graphics::GraphicsApi;

mod texture;

#[derive(Default, Debug)]
/// Marker trait for software rendering
///
/// This is the only graphics api that is guaranteed to always work anywhere
pub struct Software;

#[derive(Default, Clone, Debug)]
/// Does not actually require any extensions
pub struct Features;

impl BitOr for Features {
    type Output = Self;

    fn bitor(self, rhs: Self) -> Self::Output {
        rhs
    }
}

impl GraphicsApi for Software {
    type InitializationData = ();
    type Texture = Texture<Srgba<u8>>;
    type Requirements = Features;
}
