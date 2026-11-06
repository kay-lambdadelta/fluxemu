use std::sync::{Arc, Mutex};

use egui_wgpu::{Renderer, RendererOptions};
use fluxemu_frontend::graphics::{DisplayContext, GraphicsRuntime};
use fluxemu_frontend_egui::rendering::webgpu::Resources;
use fluxemu_graphics::{
    api::{
        GraphicsApi,
        webgpu::{InitializationData, Webgpu, suggested_framebuffer_texture_format},
    },
    shader::ShaderType,
    texture::{CowTexture, OwnedTexture},
};
use fluxemu_runtime::graphics::GraphicsRequirements;
use nalgebra::Vector2;
use palette::Srgba;
use pollster::FutureExt;
use wgpu::{
    BufferDescriptor, BufferUsages, COPY_BYTES_PER_ROW_ALIGNMENT, CommandEncoderDescriptor,
    CreateSurfaceError, Device, DeviceDescriptor, ExperimentalFeatures, Instance, MapMode,
    MemoryHints, PollType, Queue, Surface, TexelCopyBufferInfo, TexelCopyBufferLayout, Trace,
    util::initialize_adapter_from_env_or_default,
};

#[cfg(feature = "windowing")]
mod windowing;

#[cfg(feature = "drm")]
mod drm;

#[cfg(feature = "egui")]
mod egui;

pub struct WebgpuGraphicsRuntime<D> {
    configuration_dependent_data: Option<ConfigurationDependentData>,
    gpu_submission_lock: Arc<Mutex<()>>,
    display_handle: D,
}

impl<D: WebgpuCompatibleDisplayContext> GraphicsRuntime for WebgpuGraphicsRuntime<D> {
    type GraphicsApi = Webgpu;

    fn reconfigure(&mut self, graphics_requirements: GraphicsRequirements<Self::GraphicsApi>) {
        // Drop old data
        drop(self.configuration_dependent_data.take().unwrap());

        self.configuration_dependent_data = Some(ConfigurationDependentData::new(
            self.display_handle.dimensions(),
            graphics_requirements,
            &self.display_handle,
        ));
    }

    fn refresh_surface(&mut self) {
        let size = self.display_handle.dimensions();
        let ConfigurationDependentData {
            device, surface, ..
        } = self.configuration_dependent_data.as_mut().unwrap();

        let mut surface_config = surface.get_configuration().unwrap();
        surface_config.width = size.x;
        surface_config.height = size.y;

        surface.configure(device, &surface_config);
    }

    fn component_initialization_data(
        &self,
    ) -> <Self::GraphicsApi as GraphicsApi>::InitializationData {
        let ConfigurationDependentData { device, queue, .. } =
            self.configuration_dependent_data.as_ref().unwrap();

        InitializationData {
            device: device.clone(),
            queue: queue.clone(),
            gpu_submission_lock: self.gpu_submission_lock.clone(),
        }
    }

    fn max_texture_side(&self) -> u32 {
        self.configuration_dependent_data
            .as_ref()
            .unwrap()
            .device
            .limits()
            .max_texture_dimension_2d
    }

    fn screenshot<'a>(
        &self,
        framebuffer: &'a <Self::GraphicsApi as GraphicsApi>::Framebuffer,
    ) -> Option<CowTexture<'a, Srgba<u8>>> {
        if framebuffer.format() != suggested_framebuffer_texture_format() {
            return None;
        }

        let ConfigurationDependentData { device, queue, .. } =
            self.configuration_dependent_data.as_ref().unwrap();

        let width = framebuffer.width() as usize;
        let height = framebuffer.height() as usize;
        let row_length = width * size_of::<Srgba<u8>>();
        let padded_row_length = row_length.next_multiple_of(COPY_BYTES_PER_ROW_ALIGNMENT as usize);

        let buffer = device.create_buffer(&BufferDescriptor {
            label: None,
            size: (padded_row_length * height) as u64,
            usage: BufferUsages::COPY_DST | BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });

        let mut encoder = device.create_command_encoder(&CommandEncoderDescriptor::default());
        encoder.copy_texture_to_buffer(
            framebuffer.as_image_copy(),
            TexelCopyBufferInfo {
                buffer: &buffer,
                layout: TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(padded_row_length as u32),
                    rows_per_image: None,
                },
            },
            framebuffer.size(),
        );

        {
            let _guard = self.gpu_submission_lock.lock().unwrap();
            queue.submit([encoder.finish()]);
        }

        let slice = buffer.slice(..);
        slice.map_async(MapMode::Read, |_| {});
        device.poll(PollType::wait_indefinitely()).ok()?;

        let mapped = slice.get_mapped_range().ok()?;

        let pixels: Vec<_> = mapped
            .chunks_exact(padded_row_length)
            .flat_map(|row| bytemuck::pod_collect_to_vec::<u8, Srgba<u8>>(&row[..row_length]))
            .collect();

        Some(OwnedTexture::from_storage(width, height, pixels.into_boxed_slice()).into())
    }
}

struct ConfigurationDependentData {
    device: Device,
    queue: Queue,
    renderer: Renderer,
    surface: Surface<'static>,
    framebuffer_callback_resources: Arc<Resources>,
}

impl ConfigurationDependentData {
    fn new(
        dimensions: Vector2<u32>,
        graphics_requirements: GraphicsRequirements<Webgpu>,
        display_handle: &impl WebgpuCompatibleDisplayContext,
    ) -> Self {
        let (instance, surface) = display_handle
            .produce_instance_and_surface()
            .expect("Creating instance and surface");

        let adapter = initialize_adapter_from_env_or_default(&instance, Some(&surface))
            .block_on()
            .expect("Creating adapter");

        let preferred_features =
            graphics_requirements.required.clone() | graphics_requirements.preferred.clone();

        let (device, queue) = if let Ok((device, queue)) = adapter
            .request_device(&DeviceDescriptor {
                label: None,
                required_features: preferred_features.features,
                required_limits: preferred_features.limits,
                memory_hints: MemoryHints::Performance,
                trace: Trace::Off,
                experimental_features: ExperimentalFeatures::disabled(),
            })
            .block_on()
        {
            (device, queue)
        } else if let Ok((device, queue)) = adapter
            .request_device(&DeviceDescriptor {
                label: None,
                required_features: graphics_requirements.required.features,
                required_limits: graphics_requirements.required.limits,
                memory_hints: MemoryHints::MemoryUsage,
                trace: Trace::Off,
                experimental_features: ExperimentalFeatures::disabled(),
            })
            .block_on()
        {
            (device, queue)
        } else {
            panic!("Failed to create device");
        };

        let surface_config = surface
            .get_default_config(&adapter, dimensions.x, dimensions.y)
            .unwrap();

        surface.configure(&device, &surface_config);

        let framebuffer_callback_resources = Resources::new(
            device.clone(),
            queue.clone(),
            surface_config.format,
            ShaderType::default(),
        );

        let renderer = Renderer::new(
            &device,
            surface_config.format,
            RendererOptions {
                msaa_samples: 0,
                depth_stencil_format: None,
                dithering: true,
                predictable_texture_filtering: false,
            },
        );

        ConfigurationDependentData {
            device,
            queue,
            renderer,
            surface,
            framebuffer_callback_resources,
        }
    }
}

trait WebgpuCompatibleDisplayContext: DisplayContext {
    fn produce_instance_and_surface(
        &self,
    ) -> Result<(Instance, Surface<'static>), CreateSurfaceError>;
}
