use fluxemu_runtime::graphics::{
    GraphicsApi,
    software::{Software, Texture, TextureImpl, TextureImplMut},
};
use palette::{Srgba, named::BLACK};

use super::{Chip8DisplayBackend, SupportedGraphicsApiChip8Display};
use crate::display::LORES;

#[derive(Debug)]
pub struct SoftwareState;

impl Chip8DisplayBackend for SoftwareState {
    type GraphicsApi = Software;

    fn new(_: ()) -> Self {
        Self
    }

    fn create_framebuffer(&self) -> <Self::GraphicsApi as GraphicsApi>::Framebuffer {
        Texture::new(LORES.x as usize, LORES.y as usize, BLACK.into())
    }

    fn commit_staging_buffer(
        &mut self,
        staging_buffer: &Texture<Srgba<u8>>,
        framebuffer: &mut <Self::GraphicsApi as GraphicsApi>::Framebuffer,
    ) {
        framebuffer.resize(
            staging_buffer.width(),
            staging_buffer.height(),
            BLACK.into(),
        );

        framebuffer.copy_from(staging_buffer, .., ..);
    }
}

impl SupportedGraphicsApiChip8Display for Software {
    type Backend = SoftwareState;
}
