use std::sync::{Arc, Mutex};

use egui_wgpu::{Renderer, RendererOptions};
use fluxemu_frontend::graphics::{DisplayContext, GraphicsRuntime};
use fluxemu_frontend_egui::rendering::webgpu::Resources;
use fluxemu_graphics::{
    api::{
        GraphicsApi,
        webgpu::{InitializationData, Webgpu},
    },
    texture::CowTexture,
};
use fluxemu_runtime::graphics::GraphicsRequirements;
use nalgebra::Vector2;
use palette::Srgba;
use pollster::FutureExt;
use wgpu::{
    CreateSurfaceError, Device, DeviceDescriptor, ExperimentalFeatures, Instance, MemoryHints,
    Queue, Surface, Trace, util::initialize_adapter_from_env_or_default,
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

    fn screenshot(&self) -> CowTexture<'_, Srgba<u8>> {
        todo!()
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

        let framebuffer_callback_resources =
            Resources::new(device.clone(), queue.clone(), surface_config.format);

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
