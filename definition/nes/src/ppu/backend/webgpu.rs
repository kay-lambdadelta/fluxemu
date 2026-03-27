use fluxemu_runtime::graphics::{
    GraphicsApi,
    software::{self, Texture, TextureImpl},
    webgpu::{InitializationData, Webgpu, suggested_framebuffer_texture_usages},
};
use palette::{Srgba, named::BLACK};
use wgpu::{
    Device, Extent3d, Origin3d, Queue, TexelCopyBufferLayout, TexelCopyTextureInfo, TextureAspect,
    TextureDescriptor, TextureDimension, TextureFormat,
};

use super::{PpuDisplayBackend, SupportedGraphicsApiPpu};
use crate::ppu::{
    VISIBLE_SCANLINE_LENGTH, backend::convert_paletted_staging_buffer, color::PpuColorIndex,
    region::Region,
};

#[derive(Debug)]
pub struct State {
    pub device: Device,
    pub queue: Queue,
    pub staging_texture: Texture<Srgba<u8>>,
}

impl<R: Region> PpuDisplayBackend<R> for State {
    type GraphicsApi = Webgpu;

    fn new(initialization_data: InitializationData) -> Self {
        State {
            device: initialization_data.device,
            queue: initialization_data.queue,
            staging_texture: Texture::new(
                VISIBLE_SCANLINE_LENGTH as usize,
                R::VISIBLE_SCANLINES as usize,
                BLACK.into(),
            ),
        }
    }

    fn create_framebuffer(&self) -> <Self::GraphicsApi as GraphicsApi>::Texture {
        self.device.create_texture(&TextureDescriptor {
            label: None,
            size: Extent3d {
                width: VISIBLE_SCANLINE_LENGTH as u32,
                height: R::VISIBLE_SCANLINES as u32,
                ..Default::default()
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: TextureDimension::D2,
            format: TextureFormat::Rgba8UnormSrgb,
            usage: suggested_framebuffer_texture_usages(),
            view_formats: &[],
        })
    }

    fn commit_staging_buffer(
        &mut self,
        staging_buffer: &software::Texture<PpuColorIndex>,
        framebuffer: &mut <Self::GraphicsApi as GraphicsApi>::Texture,
    ) {
        convert_paletted_staging_buffer::<R>(staging_buffer, &mut self.staging_texture);

        self.queue.write_texture(
            TexelCopyTextureInfo {
                texture: framebuffer,
                mip_level: 0,
                origin: Origin3d::default(),
                aspect: TextureAspect::All,
            },
            bytemuck::cast_slice(self.staging_texture.as_slice()),
            TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some((staging_buffer.width() * size_of::<Srgba<u8>>()) as u32),
                rows_per_image: None,
            },
            framebuffer.size(),
        );
    }
}

impl SupportedGraphicsApiPpu for Webgpu {
    type Backend<R: Region> = State;
}
