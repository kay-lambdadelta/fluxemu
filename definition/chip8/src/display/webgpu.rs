use fluxemu_graphics::api::{
    GraphicsApi,
    software::texture::OwnedTexture,
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
    pub staging_texture: OwnedTexture<Srgba<u8>>,
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
            staging_texture: OwnedTexture::new(LORES.x as usize, LORES.y as usize),
        }
    }

    fn framebuffer(&mut self) -> &<Self::GraphicsApi as GraphicsApi>::Framebuffer {
        if self.staging_texture.width() != self.framebuffer.width() as usize
            || self.staging_texture.height() != self.framebuffer.height() as usize
        {
            let new_framebuffer = self.device.create_texture(&TextureDescriptor {
                label: None,
                size: Extent3d {
                    width: self.staging_texture.width() as u32,
                    height: self.staging_texture.height() as u32,
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
            bytemuck::cast_slice(self.staging_texture.as_slice().unwrap()),
            TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some((self.staging_texture.width() * size_of::<Srgba<u8>>()) as u32),
                rows_per_image: None,
            },
            self.framebuffer.size(),
        );

        &self.framebuffer
    }

    fn commit_staging_buffer(&mut self, staging_buffer: &OwnedTexture<Srgba<u8>>) {
        self.staging_texture = staging_buffer.clone();
    }
}

impl SupportedGraphicsApiChip8Display for Webgpu {
    type Backend = State;
}
