use std::sync::{Arc, Mutex};

use fluxemu_graphics::{
    api::{
        GraphicsApi,
        webgpu::{
            Webgpu, suggested_framebuffer_texture_format, suggested_framebuffer_texture_usages,
        },
    },
    texture::RefTexture,
};
use fluxemu_runtime::graphics::SimpleDisplayBackend;
use palette::Srgba;
use wgpu::{
    Device, Extent3d, Origin3d, Queue, TexelCopyBufferLayout, TexelCopyTextureInfo, TextureAspect,
    TextureDescriptor, TextureDimension, TextureFormat,
};

use super::{LORES, SupportedGraphicsApi};

#[derive(Debug)]
pub struct State {
    queue: Queue,
    device: Device,
    gpu_submission_lock: Arc<Mutex<()>>,
}

impl SimpleDisplayBackend for State {
    type GraphicsApi = Webgpu;

    fn new(initialization_data: <Self::GraphicsApi as GraphicsApi>::InitializationData) -> Self {
        Self {
            queue: initialization_data.queue,
            device: initialization_data.device,
            gpu_submission_lock: initialization_data.gpu_submission_lock,
        }
    }

    fn produce_initial_framebuffer(
        &mut self,
        _path: &fluxemu_runtime::ResourcePath,
    ) -> <Self::GraphicsApi as GraphicsApi>::Framebuffer {
        self.device.create_texture(&TextureDescriptor {
            label: None,
            size: Extent3d {
                width: LORES.x as u32,
                height: LORES.y as u32,
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

    fn commit_staging_buffer(
        &mut self,
        staging_buffer: RefTexture<Srgba<u8>>,
        framebuffer: &mut <Self::GraphicsApi as GraphicsApi>::Framebuffer,
    ) {
        if staging_buffer.width() != framebuffer.width() as usize
            || staging_buffer.height() != framebuffer.height() as usize
        {
            let new_framebuffer = self.device.create_texture(&TextureDescriptor {
                label: None,
                size: Extent3d {
                    width: staging_buffer.width() as u32,
                    height: staging_buffer.height() as u32,
                    ..Default::default()
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: TextureDimension::D2,
                format: TextureFormat::Rgba8UnormSrgb,
                usage: suggested_framebuffer_texture_usages(),
                view_formats: &[],
            });

            *framebuffer = new_framebuffer;
        }

        let _guard = self.gpu_submission_lock.lock().unwrap();
        self.queue.write_texture(
            TexelCopyTextureInfo {
                texture: framebuffer,
                mip_level: 0,
                origin: Origin3d::ZERO,
                aspect: TextureAspect::All,
            },
            bytemuck::cast_slice(staging_buffer.as_slice().unwrap()),
            TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some((staging_buffer.width() * size_of::<Srgba<u8>>()) as u32),
                rows_per_image: None,
            },
            framebuffer.size(),
        );
        self.queue.submit([]);
    }
}

impl SupportedGraphicsApi for Webgpu {
    type Backend = State;
}
