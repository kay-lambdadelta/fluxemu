use fluxemu_runtime::graphics::{
    GraphicsApi,
    software::{CopyMode, Software, Texture, TextureImpl, TextureImplMut},
};
use palette::{Srgba, named::BLACK};

use super::{Chip8DisplayBackend, SupportedGraphicsApiChip8Display};
use crate::display::LORES;

#[derive(Debug)]
pub struct SoftwareState {
    framebuffer: Texture<Srgba<u8>>,
}

impl Chip8DisplayBackend for SoftwareState {
    type GraphicsApi = Software;

    fn new(_: ()) -> Self {
        Self {
            framebuffer: Texture::new(LORES.x as usize, LORES.y as usize, BLACK.into()),
        }
    }

    fn framebuffer(&self) -> &<Self::GraphicsApi as GraphicsApi>::Framebuffer {
        &self.framebuffer
    }

    fn commit_staging_buffer(&mut self, staging_buffer: &Texture<Srgba<u8>>) {
        if self.framebuffer.size() != staging_buffer.size() {
            self.framebuffer = staging_buffer.clone();
        } else {
            self.framebuffer
                .copy_from(staging_buffer, CopyMode::Nearest);
        }
    }
}

impl SupportedGraphicsApiChip8Display for Software {
    type Backend = SoftwareState;
}
