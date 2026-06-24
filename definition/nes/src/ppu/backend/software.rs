use std::fmt::Debug;

use fluxemu_graphics::api::{
    GraphicsApi,
    software::{
        Software,
        texture::{AsViewTextureMut, OwnedTexture, Texture},
    },
};
use palette::{Srgba, named::BLACK};

use super::{PpuDisplayBackend, SupportedGraphicsApiPpu};
use crate::ppu::{
    VISIBLE_SCANLINE_LENGTH, backend::convert_paletted_staging_buffer, color::PpuColorIndex,
    region::Region,
};

pub struct SoftwareState {
    framebuffer: OwnedTexture<Srgba<u8>>,
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
            framebuffer: Texture::from_value(
                VISIBLE_SCANLINE_LENGTH as usize,
                R::VISIBLE_SCANLINES as usize,
                BLACK.into(),
            ),
        }
    }

    fn framebuffer(&self) -> &<Self::GraphicsApi as GraphicsApi>::Framebuffer {
        &self.framebuffer
    }

    fn commit_staging_buffer(&mut self, staging_buffer: &OwnedTexture<PpuColorIndex>) {
        convert_paletted_staging_buffer::<R>(staging_buffer, self.framebuffer.as_view_mut());
    }
}

impl SupportedGraphicsApiPpu for Software {
    type Backend<R: Region> = SoftwareState;
}
