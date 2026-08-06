use fluxemu_graphics::api::{
    GraphicsApi,
    software::texture::{CopyMode, OwnedTexture},
    webgpu::{InitializationData, Webgpu, suggested_framebuffer_texture_usages},
};
use palette::Srgba;
use wgpu::{
    Extent3d, Origin3d, Queue, TexelCopyBufferLayout, TexelCopyTextureInfo, Texture, TextureAspect,
    TextureDescriptor, TextureDimension, TextureFormat,
};

use crate::tia::{
    SupportedGraphicsApiTia, VISIBLE_SCANLINE_LENGTH, backend::TiaDisplayBackend, region::Region,
};

#[derive(Debug)]
pub struct State {
    queue: Queue,
    framebuffer: Texture,
    staging_texture: OwnedTexture<Srgba<u8>>,
}

impl<R: Region> TiaDisplayBackend<R> for State {
    type GraphicsApi = Webgpu;

    fn new(initialization_data: InitializationData) -> Self {
        let framebuffer = initialization_data
            .device
            .create_texture(&TextureDescriptor {
                label: None,
                size: Extent3d {
                    width: VISIBLE_SCANLINE_LENGTH as u32,
                    height: R::TOTAL_SCANLINES as u32,
                    ..Default::default()
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: TextureDimension::D2,
                format: TextureFormat::Rgba8UnormSrgb,
                usage: suggested_framebuffer_texture_usages(),
                view_formats: &[],
            });

        State {
            queue: initialization_data.queue,
            framebuffer,
            staging_texture: OwnedTexture::new(
                VISIBLE_SCANLINE_LENGTH as usize,
                R::TOTAL_SCANLINES as usize,
            ),
        }
    }

    fn framebuffer(&mut self) -> &<Self::GraphicsApi as GraphicsApi>::Framebuffer {
        self.queue.write_texture(
            TexelCopyTextureInfo {
                texture: &self.framebuffer,
                mip_level: 0,
                origin: Origin3d::default(),
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
        self.staging_texture
            .copy_from(staging_buffer, CopyMode::Nearest);
    }
}

impl SupportedGraphicsApiTia for Webgpu {
    type Backend<R: Region> = State;
}
