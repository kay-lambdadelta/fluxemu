use std::fmt::Debug;

use fluxemu_runtime::graphics::{
    GraphicsApi,
    software::{CopyMode, Software, Texture, TextureImplMut},
};
use palette::{Srgba, named::BLACK};

use super::{SupportedGraphicsApiTia, TiaDisplayBackend};
use crate::tia::{VISIBLE_SCANLINE_LENGTH, region::Region};

pub struct SoftwareState {
    framebuffer: Texture<Srgba<u8>>,
}

// elide the buffers

impl Debug for SoftwareState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SoftwareState").finish()
    }
}

impl<R: Region> TiaDisplayBackend<R> for SoftwareState {
    type GraphicsApi = Software;

    fn new(_: ()) -> Self {
        SoftwareState {
            framebuffer: Texture::new(
                VISIBLE_SCANLINE_LENGTH as usize,
                R::TOTAL_SCANLINES as usize,
                BLACK.into(),
            ),
        }
    }

    fn framebuffer(&self) -> &<Self::GraphicsApi as GraphicsApi>::Framebuffer {
        &self.framebuffer
    }

    fn commit_staging_buffer(&mut self, staging_buffer: &Texture<Srgba<u8>>) {
        self.framebuffer
            .copy_from(staging_buffer, CopyMode::Nearest);
    }
}

impl SupportedGraphicsApiTia for Software {
    type Backend<R: Region> = SoftwareState;
}
