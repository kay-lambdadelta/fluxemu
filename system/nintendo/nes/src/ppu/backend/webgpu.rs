use core::marker::PhantomData;

use fluxemu_graphics::{
    api::{
        GraphicsApi,
        webgpu::{
            Webgpu, suggested_framebuffer_texture_format, suggested_framebuffer_texture_usages,
        },
    },
    texture::RefTexture,
};
use fluxemu_runtime::{
    ResourcePath,
    graphics::{Handle, SimpleDisplayBackend, StagingPool, Submitter},
};
use palette::Srgba;
use wgpu::{
    Extent3d, Origin3d, TexelCopyBufferLayout, TexelCopyTextureInfo, TextureAspect,
    TextureDescriptor, TextureDimension,
};

use crate::ppu::{VISIBLE_SCANLINE_LENGTH, region::Region};

#[derive(Debug)]
pub struct State<R: Region> {
    submitter: Submitter<Webgpu>,
    staging_pool: StagingPool<Srgba<u8>>,
    _phantom: PhantomData<fn() -> R>,
}

impl<R: Region> State<R> {
    pub fn new(submitter: Submitter<Webgpu>) -> Self {
        State {
            submitter,
            staging_pool: StagingPool::default(),
            _phantom: PhantomData,
        }
    }
}

impl<R: Region> SimpleDisplayBackend for State<R> {
    type GraphicsApi = Webgpu;

    fn produce_initial_framebuffer(
        &mut self,
        _path: &ResourcePath,
    ) -> Handle<<Self::GraphicsApi as GraphicsApi>::Framebuffer> {
        let framebuffer = self.submitter.reserve();
        let size = Extent3d {
            width: VISIBLE_SCANLINE_LENGTH as u32,
            height: R::VISIBLE_SCANLINES as u32,
            ..Default::default()
        };

        self.submitter.submit(move |gpu| {
            let texture = gpu.context.device.create_texture(&TextureDescriptor {
                label: None,
                size,
                mip_level_count: 1,
                sample_count: 1,
                dimension: TextureDimension::D2,
                format: suggested_framebuffer_texture_format(),
                usage: suggested_framebuffer_texture_usages(),
                view_formats: &[],
            });

            gpu.resources.insert(framebuffer, texture);
        });

        framebuffer
    }

    fn commit_staging_buffer(
        &mut self,
        staging_buffer: RefTexture<Srgba<u8>>,
        framebuffer: Handle<<Self::GraphicsApi as GraphicsApi>::Framebuffer>,
    ) {
        let bytes_per_row = (staging_buffer.width() * size_of::<Srgba<u8>>()) as u32;
        let frame = self.staging_pool.copy(staging_buffer);
        let staging_pool = self.staging_pool.clone();

        self.submitter.submit_replacing(framebuffer, move |gpu| {
            let Some(texture) = gpu.resources.get(framebuffer) else {
                staging_pool.recycle(frame);

                return;
            };

            gpu.context.queue.write_texture(
                TexelCopyTextureInfo {
                    texture,
                    mip_level: 0,
                    origin: Origin3d::default(),
                    aspect: TextureAspect::All,
                },
                bytemuck::cast_slice(frame.as_slice().unwrap()),
                TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(bytes_per_row),
                    rows_per_image: None,
                },
                texture.size(),
            );
            gpu.context.queue.submit([]);
            staging_pool.recycle(frame);
        });
    }
}
