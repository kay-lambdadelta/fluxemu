use fluxemu_runtime::graphics::{
    GraphicsApi,
    software::{self, TextureImpl},
    webgpu::{Webgpu, suggested_framebuffer_texture_usages},
};
use palette::Srgba;
use wgpu::{
    Device, Extent3d, Origin3d, Queue, TexelCopyBufferLayout, TexelCopyTextureInfo, Texture,
    TextureAspect, TextureDescriptor, TextureDimension, TextureFormat,
};

use super::{LORES, SupportedGraphicsApiChip8Display};
use crate::display::Chip8DisplayBackend;

#[derive(Debug)]
pub struct State {
    pub queue: Queue,
    pub device: Device,
    pub framebuffer: Texture,
}

impl Chip8DisplayBackend for State {
    type GraphicsApi = Webgpu;

    fn new(initialization_data: <Self::GraphicsApi as GraphicsApi>::InitializationData) -> Self {
        let framebuffer = initialization_data
            .device
            .create_texture(&TextureDescriptor {
                label: None,
                size: Extent3d {
                    width: LORES.x as u32,
                    height: LORES.y as u32,
                    ..Default::default()
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: TextureDimension::D2,
                format: TextureFormat::Rgba8UnormSrgb,
                usage: suggested_framebuffer_texture_usages(),
                view_formats: &[],
            });

        Self {
            queue: initialization_data.queue,
            device: initialization_data.device,
            framebuffer,
        }
    }

    fn framebuffer(&self) -> &<Self::GraphicsApi as GraphicsApi>::Framebuffer {
        &self.framebuffer
    }

    fn commit_staging_buffer(&mut self, staging_buffer: &software::Texture<Srgba<u8>>) {
        if staging_buffer.width() != self.framebuffer.width() as usize
            || staging_buffer.height() != self.framebuffer.height() as usize
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

            self.framebuffer = new_framebuffer;
        }

        self.queue.write_texture(
            TexelCopyTextureInfo {
                texture: &self.framebuffer,
                mip_level: 0,
                origin: Origin3d::ZERO,
                aspect: TextureAspect::All,
            },
            bytemuck::cast_slice(staging_buffer.as_slice()),
            TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some((staging_buffer.width() * size_of::<Srgba<u8>>()) as u32),
                rows_per_image: None,
            },
            self.framebuffer.size(),
        );
    }
}

impl SupportedGraphicsApiChip8Display for Webgpu {
    type Backend = State;
}
