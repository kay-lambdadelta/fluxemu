use std::{
    marker::PhantomData,
    sync::{Arc, Mutex},
};

use fluxemu_graphics::{
    api::{
        GraphicsApi,
        webgpu::{
            InitializationData, Webgpu, suggested_framebuffer_texture_format,
            suggested_framebuffer_texture_usages,
        },
    },
    texture::RefTexture,
};
use fluxemu_runtime::{ResourcePath, graphics::SimpleDisplayBackend};
use palette::Srgba;
use wgpu::{
    Device, Extent3d, Origin3d, Queue, TexelCopyBufferLayout, TexelCopyTextureInfo, TextureAspect,
    TextureDescriptor, TextureDimension,
};

use crate::tia::{SupportedGraphicsApi, VISIBLE_SCANLINE_LENGTH, region::Region};

#[derive(Debug)]
pub struct State<R: Region> {
    queue: Queue,
    device: Device,
    gpu_submission_lock: Arc<Mutex<()>>,
    _phantom: PhantomData<fn() -> R>,
}

impl<R: Region> SimpleDisplayBackend for State<R> {
    type GraphicsApi = Webgpu;

    fn new(initialization_data: InitializationData) -> Self {
        State {
            queue: initialization_data.queue,
            device: initialization_data.device,
            gpu_submission_lock: initialization_data.gpu_submission_lock,
            _phantom: PhantomData,
        }
    }

    fn produce_initial_framebuffer(
        &mut self,
        _path: &ResourcePath,
    ) -> <Self::GraphicsApi as GraphicsApi>::Framebuffer {
        self.device.create_texture(&TextureDescriptor {
            label: None,
            size: Extent3d {
                width: VISIBLE_SCANLINE_LENGTH as u32,
                height: R::TOTAL_SCANLINES as u32,
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
        let _guard = self.gpu_submission_lock.lock().unwrap();
        self.queue.write_texture(
            TexelCopyTextureInfo {
                texture: framebuffer,
                mip_level: 0,
                origin: Origin3d::default(),
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
    type Backend<R: Region> = State<R>;
}
