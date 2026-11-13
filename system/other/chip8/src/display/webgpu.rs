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
    Device, Extent3d, Origin3d, TexelCopyBufferLayout, TexelCopyTextureInfo, Texture,
    TextureAspect, TextureDescriptor, TextureDimension,
};

use super::LORES;

#[derive(Debug)]
pub struct State {
    submitter: Submitter<Webgpu>,
    staging_pool: StagingPool<Srgba<u8>>,
}

impl State {
    pub fn new(submitter: Submitter<Webgpu>) -> Self {
        Self {
            submitter,
            staging_pool: StagingPool::default(),
        }
    }
}

impl SimpleDisplayBackend for State {
    type GraphicsApi = Webgpu;

    fn produce_initial_framebuffer(
        &mut self,
        _path: &ResourcePath,
    ) -> Handle<<Self::GraphicsApi as GraphicsApi>::Framebuffer> {
        let framebuffer = self.submitter.reserve();

        self.submitter.submit(move |gpu| {
            let texture = create_framebuffer(&gpu.context.device, LORES.x as u32, LORES.y as u32);

            gpu.resources.insert(framebuffer, texture);
        });

        framebuffer
    }

    fn commit_staging_buffer(
        &mut self,
        staging_buffer: RefTexture<Srgba<u8>>,
        framebuffer: Handle<<Self::GraphicsApi as GraphicsApi>::Framebuffer>,
    ) {
        let width = staging_buffer.width() as u32;
        let height = staging_buffer.height() as u32;
        let frame = self.staging_pool.copy(staging_buffer);
        let staging_pool = self.staging_pool.clone();

        self.submitter.submit_replacing(framebuffer, move |gpu| {
            let resized = gpu
                .resources
                .get(framebuffer)
                .is_none_or(|texture| texture.width() != width || texture.height() != height);

            if resized {
                let texture = create_framebuffer(&gpu.context.device, width, height);

                gpu.resources.insert(framebuffer, texture);
            }

            let texture = gpu.resources.get(framebuffer).unwrap();

            gpu.context.queue.write_texture(
                TexelCopyTextureInfo {
                    texture,
                    mip_level: 0,
                    origin: Origin3d::ZERO,
                    aspect: TextureAspect::All,
                },
                bytemuck::cast_slice(frame.as_slice().unwrap()),
                TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(width * size_of::<Srgba<u8>>() as u32),
                    rows_per_image: None,
                },
                texture.size(),
            );
            gpu.context.queue.submit([]);
            staging_pool.recycle(frame);
        });
    }
}

fn create_framebuffer(device: &Device, width: u32, height: u32) -> Texture {
    device.create_texture(&TextureDescriptor {
        label: None,
        size: Extent3d {
            width,
            height,
            ..Default::default()
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: TextureDimension::D2,
        format: suggested_framebuffer_texture_format(),
        usage: suggested_framebuffer_texture_usages(),
        view_formats: &[],
    })
}
