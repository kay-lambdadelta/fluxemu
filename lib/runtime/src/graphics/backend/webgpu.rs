use std::ops::BitOr;

use wgpu::{Device, Features, Limits, Queue, Texture, TextureUsages};

use crate::graphics::GraphicsApi;

#[derive(Default, Debug)]
pub struct Webgpu;

#[derive(Debug, Clone)]
pub struct InitializationData {
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

impl GraphicsApi for Webgpu {
    type InitializationData = InitializationData;
    type Texture = Texture;
    type Requirements = Requirements;
}

/// Texture usages that any implementation of the webgpu api will probably use
#[must_use]
pub fn suggested_framebuffer_texture_usages() -> TextureUsages {
    TextureUsages::COPY_DST | TextureUsages::COPY_SRC | TextureUsages::TEXTURE_BINDING
}
