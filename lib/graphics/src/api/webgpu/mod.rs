use core::ops::BitOr;

use alloc::sync::Arc;
use wgpu::{Device, Features, Limits, Queue, Texture, TextureFormat, TextureUsages};

use crate::api::{Framebuffer, GraphicsApi};

use std::sync::Mutex;

extern crate std;

pub mod shader;

#[derive(Default, Debug)]
pub struct Webgpu;

#[derive(Debug, Clone)]
pub struct InitializationData {
    pub device: Device,
    pub queue: Queue,
    pub gpu_submission_lock: Arc<Mutex<()>>,
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
    type Framebuffer = Texture;
    type InitializationData = InitializationData;
    type Requirements = Requirements;
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
