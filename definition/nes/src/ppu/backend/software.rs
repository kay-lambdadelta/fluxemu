use std::fmt::Debug;

use fluxemu_runtime::graphics::{
    GraphicsApi,
    software::{Software, Texture},
};
use palette::{Srgba, named::BLACK};

use super::{PpuDisplayBackend, SupportedGraphicsApiPpu};
use crate::ppu::{
    VISIBLE_SCANLINE_LENGTH, backend::convert_paletted_staging_buffer, color::PpuColorIndex,
    region::Region,
};

pub struct SoftwareState {
    framebuffer: Texture<Srgba<u8>>,
}

// elide the buffers

impl Debug for SoftwareState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SoftwareState").finish()
    }
}

impl<R: Region> PpuDisplayBackend<R> for SoftwareState {
    type GraphicsApi = Software;

    fn new(_: ()) -> Self {
        SoftwareState {
            framebuffer: Texture::new(
                VISIBLE_SCANLINE_LENGTH as usize,
                R::VISIBLE_SCANLINES as usize,
                BLACK.into(),
            ),
        }
    }

    fn framebuffer(&self) -> &<Self::GraphicsApi as GraphicsApi>::Framebuffer {
        &self.framebuffer
    }

    fn commit_staging_buffer(&mut self, staging_buffer: &Texture<PpuColorIndex>) {
        convert_paletted_staging_buffer::<R>(staging_buffer, &mut self.framebuffer);
    }
}

impl SupportedGraphicsApiPpu for Software {
    type Backend<R: Region> = SoftwareState;
}
