use std::fmt::Debug;

use fluxemu_runtime::graphics::{
    GraphicsApi,
    software::{Software, Texture, TextureImplMut},
};
use palette::{Srgba, named::BLACK};

use super::{PpuDisplayBackend, SupportedGraphicsApiPpu};
use crate::ppu::{VISIBLE_SCANLINE_LENGTH, region::Region};

pub struct SoftwareState;

// elide the buffers

impl Debug for SoftwareState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SoftwareState").finish()
    }
}

impl<R: Region> PpuDisplayBackend<R> for SoftwareState {
    type GraphicsApi = Software;

    fn new(_: ()) -> Self {
        SoftwareState
    }

    fn create_framebuffer(&self) -> <Self::GraphicsApi as GraphicsApi>::Texture {
        Texture::new(
            VISIBLE_SCANLINE_LENGTH as usize,
            R::VISIBLE_SCANLINES as usize,
            BLACK.into(),
        )
    }

    fn commit_staging_buffer(
        &mut self,
        staging_buffer: &Texture<Srgba<u8>>,
        framebuffer: &mut <Self::GraphicsApi as GraphicsApi>::Texture,
    ) {
        framebuffer.copy_from(staging_buffer, .., ..);
    }
}

impl SupportedGraphicsApiPpu for Software {
    type Backend<R: Region> = SoftwareState;
}
