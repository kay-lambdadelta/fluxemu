use std::{fmt::Debug, marker::PhantomData};

use fluxemu_graphics::{
    api::{GraphicsApi, software::Software},
    texture::{CopyMode, RefTexture, Texture},
};
use fluxemu_runtime::{ResourcePath, graphics::SimpleDisplayBackend};
use palette::{Srgba, named::BLACK};

use crate::ppu::{VISIBLE_SCANLINE_LENGTH, backend::SupportedGraphicsApi, region::Region};

#[derive(Debug)]
pub struct State<R: Region> {
    _phantom: PhantomData<fn() -> R>,
}

impl<R: Region> SimpleDisplayBackend for State<R> {
    type GraphicsApi = Software;

    fn new(_: ()) -> Self {
        State {
            _phantom: PhantomData,
        }
    }

    fn produce_initial_framebuffer(
        &mut self,
        _path: &ResourcePath,
    ) -> <Self::GraphicsApi as GraphicsApi>::Framebuffer {
        Texture::from_value(
            VISIBLE_SCANLINE_LENGTH as usize,
            R::VISIBLE_SCANLINES as usize,
            BLACK.into(),
        )
    }

    fn commit_staging_buffer(
        &mut self,
        staging_buffer: RefTexture<Srgba<u8>>,
        framebuffer: &mut <Self::GraphicsApi as GraphicsApi>::Framebuffer,
    ) {
        framebuffer.copy_from(staging_buffer, CopyMode::Nearest);
    }
}

impl SupportedGraphicsApi for Software {
    type Backend<R: Region> = State<R>;
}
