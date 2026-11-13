use alloc::{borrow::Cow, vec::Vec};
use core::ops::BitOr;

use nalgebra::Vector2;
use palette::Srgba;
use wgpu::{
    Device, Extent3d, Features, Limits, Origin3d, Queue, TexelCopyBufferLayout,
    TexelCopyTextureInfo, Texture, TextureAspect, TextureDescriptor, TextureDimension,
    TextureFormat, TextureUsages,
};

use crate::{
    api::{Framebuffer, GraphicsApi},
    texture::RefTexture,
};

pub mod shader;

#[derive(Default, Debug)]
pub struct Webgpu;

#[derive(Debug, Clone)]
pub struct Context {
    pub device: Device,
    pub queue: Queue,
}

#[derive(Debug, Clone)]
pub struct Requirements {
    pub features: Features,
    pub limits: Limits,
}

impl Default for Requirements {
    fn default() -> Self {
        Self {
            features: Features::empty(),
            limits: Limits::downlevel_defaults(),
        }
    }
}

impl BitOr for Requirements {
    type Output = Self;

    fn bitor(self, other: Self) -> Self {
        Self {
            features: self.features | other.features,
            limits: self.limits.or_better_values_from(&other.limits),
        }
    }
}

impl Framebuffer for Texture {
    fn width(&self) -> usize {
        self.width() as usize
    }

    fn height(&self) -> usize {
        self.height() as usize
    }
}

impl GraphicsApi for Webgpu {
    type Context = Context;
    type Framebuffer = Texture;
    type Requirements = Requirements;

    const NAME: &'static str = "WebGPU";

    fn create_software_framebuffer(context: &mut Context, size: Vector2<usize>) -> Texture {
        context.device.create_texture(&TextureDescriptor {
            label: None,
            size: Extent3d {
                width: size.x as u32,
                height: size.y as u32,
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

    fn upload_software_frame(
        context: &mut Context,
        framebuffer: &mut Texture,
        frame: RefTexture<'_, Srgba<u8>>,
    ) {
        let pixels = match frame.as_slice() {
            Some(pixels) => Cow::Borrowed(pixels),
            None => Cow::Owned(frame.iter_pixels().copied().collect::<Vec<_>>()),
        };

        context.queue.write_texture(
            TexelCopyTextureInfo {
                texture: framebuffer,
                mip_level: 0,
                origin: Origin3d::ZERO,
                aspect: TextureAspect::All,
            },
            bytemuck::cast_slice(&pixels),
            TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some((frame.width() * size_of::<Srgba<u8>>()) as u32),
                rows_per_image: None,
            },
            framebuffer.size(),
        );
        context.queue.submit([]);
    }
}

/// Texture usages that any implementation of the webgpu api will probably use
#[must_use]
pub fn suggested_framebuffer_texture_usages() -> TextureUsages {
    TextureUsages::COPY_DST | TextureUsages::COPY_SRC | TextureUsages::TEXTURE_BINDING
}

#[must_use]
pub fn suggested_framebuffer_texture_format() -> TextureFormat {
    TextureFormat::Rgba8UnormSrgb
}
