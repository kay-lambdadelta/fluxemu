use fluxemu_graphics::{
    api::{GraphicsApi, software::Software},
    texture::{CopyMode, RefTexture, Texture},
};
use fluxemu_runtime::{ResourcePath, graphics::SimpleDisplayBackend};
use palette::{Srgba, named::BLACK};

use super::SupportedGraphicsApi;
use crate::display::LORES;

#[derive(Debug)]
pub struct State;

impl SimpleDisplayBackend for State {
    type GraphicsApi = Software;

    fn new(_: ()) -> Self {
        Self
    }

    fn produce_initial_framebuffer(
        &mut self,
        _path: &ResourcePath,
    ) -> <Self::GraphicsApi as GraphicsApi>::Framebuffer {
        Texture::from_value(LORES.x as usize, LORES.y as usize, BLACK.into())
    }

    fn commit_staging_buffer(
        &mut self,
        staging_buffer: RefTexture<Srgba<u8>>,
        framebuffer: &mut <Self::GraphicsApi as GraphicsApi>::Framebuffer,
    ) {
        if framebuffer.size() != staging_buffer.size() {
            *framebuffer = Texture::from_value(
                staging_buffer.width(),
                staging_buffer.height(),
                BLACK.into(),
            );
        }

        framebuffer.copy_from(staging_buffer, CopyMode::Nearest);
    }
}

impl SupportedGraphicsApi for Software {
    type Backend = State;
}
