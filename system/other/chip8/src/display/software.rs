use fluxemu_graphics::{
    api::{GraphicsApi, software::Software},
    texture::{RefTexture, Texture},
};
use fluxemu_runtime::{
    ResourcePath,
    graphics::{Handle, SimpleDisplayBackend, StagingPool, Submitter},
};
use palette::{Srgba, named::BLACK};

use crate::display::LORES;

#[derive(Debug)]
pub struct State {
    submitter: Submitter<Software>,
    staging_pool: StagingPool<Srgba<u8>>,
}

impl State {
    pub fn new(submitter: Submitter<Software>) -> Self {
        Self {
            submitter,
            staging_pool: StagingPool::default(),
        }
    }
}

impl SimpleDisplayBackend for State {
    type GraphicsApi = Software;

    fn produce_initial_framebuffer(
        &mut self,
        _path: &ResourcePath,
    ) -> Handle<<Self::GraphicsApi as GraphicsApi>::Framebuffer> {
        let framebuffer = self.submitter.reserve();
        let initial = Texture::from_value(LORES.x as usize, LORES.y as usize, BLACK.into());

        self.submitter.submit(move |gpu| {
            gpu.resources.insert(framebuffer, initial);
        });

        framebuffer
    }

    fn commit_staging_buffer(
        &mut self,
        staging_buffer: RefTexture<Srgba<u8>>,
        framebuffer: Handle<<Self::GraphicsApi as GraphicsApi>::Framebuffer>,
    ) {
        let frame = self.staging_pool.copy(staging_buffer);
        let staging_pool = self.staging_pool.clone();

        self.submitter.submit_replacing(framebuffer, move |gpu| {
            if let Some(previous) = gpu.resources.insert(framebuffer, frame) {
                staging_pool.recycle(previous);
            }
        });
    }
}
