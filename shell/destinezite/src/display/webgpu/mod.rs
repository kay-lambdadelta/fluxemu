use egui_wgpu::{Renderer, RendererOptions};
use fluxemu_frontend::graphics::{DisplayContext, GraphicsRuntime};
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
    Adapter, BindGroupLayout, BindGroupLayoutDescriptor, BindGroupLayoutEntry, BindingType,
    BlendState, Buffer, BufferBindingType, BufferDescriptor, BufferUsages, ColorTargetState,
    ColorWrites, CreateSurfaceError, Device, DeviceDescriptor, ExperimentalFeatures, FilterMode,
    FragmentState, Instance, MemoryHints, MultisampleState, PipelineCompilationOptions,
    PipelineLayoutDescriptor, PrimitiveState, Queue, RenderPipeline, RenderPipelineDescriptor,
    Sampler, SamplerBindingType, SamplerDescriptor, ShaderModuleDescriptor, ShaderSource,
    ShaderStages, Surface, TextureSampleType, TextureViewDimension, Trace, VertexState,
    util::initialize_adapter_from_env_or_default,
};

use crate::display::webgpu::{
    egui::find_egui_texture_view_format,
    shader::{NORMAL_SHADER, ShaderUniform},
};

#[cfg(feature = "windowing")]
mod windowing;

#[cfg(feature = "drm")]
mod drm;

#[cfg(feature = "egui")]
mod egui;

mod shader;

pub struct WebgpuGraphicsRuntime<D> {
    configuration_dependent_data: Option<ConfigurationDependentData>,
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
    adapter: Adapter,
    device: Device,
    queue: Queue,
    bind_group_layout: BindGroupLayout,
    pipeline: RenderPipeline,
    uniform_buffer: Buffer,
    machine_draw_sampler: Sampler,
    renderer: Renderer,
    surface: Surface<'static>,
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

        let mut surface_config = surface
            .get_default_config(&adapter, dimensions.x, dimensions.y)
            .unwrap();

        let egui_texture_view_format = find_egui_texture_view_format(
            surface_config.format,
            adapter.get_downlevel_capabilities(),
        );

        if surface_config.format != egui_texture_view_format {
            surface_config.view_formats.push(egui_texture_view_format);
        }

        surface.configure(&device, &surface_config);

        let shader = device.create_shader_module(ShaderModuleDescriptor {
            label: None,
            source: ShaderSource::Wgsl(NORMAL_SHADER.into()),
        });

        let bind_group_layout = device.create_bind_group_layout(&BindGroupLayoutDescriptor {
            label: None,
            entries: &[
                BindGroupLayoutEntry {
                    binding: 0,
                    visibility: ShaderStages::VERTEX_FRAGMENT,
                    ty: BindingType::Buffer {
                        ty: BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                BindGroupLayoutEntry {
                    binding: 1,
                    visibility: ShaderStages::FRAGMENT,
                    ty: BindingType::Texture {
                        multisampled: false,
                        view_dimension: TextureViewDimension::D2,
                        sample_type: TextureSampleType::Float { filterable: true },
                    },
                    count: None,
                },
                BindGroupLayoutEntry {
                    binding: 2,
                    visibility: ShaderStages::FRAGMENT,
                    ty: BindingType::Sampler(SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });

        let pipeline_layout = device.create_pipeline_layout(&PipelineLayoutDescriptor {
            label: None,
            bind_group_layouts: &[Some(&bind_group_layout)],
            immediate_size: 0,
        });

        let pipeline = device.create_render_pipeline(&RenderPipelineDescriptor {
            label: None,
            layout: Some(&pipeline_layout),
            vertex: VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                buffers: &[],
                compilation_options: PipelineCompilationOptions::default(),
            },
            fragment: Some(FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                targets: &[Some(ColorTargetState {
                    format: surface_config.format,
                    blend: Some(BlendState::REPLACE),
                    write_mask: ColorWrites::ALL,
                })],
                compilation_options: PipelineCompilationOptions::default(),
            }),
            primitive: PrimitiveState::default(),
            depth_stencil: None,
            multisample: MultisampleState::default(),
            multiview_mask: None,
            cache: None,
        });

        let uniform_buffer = device.create_buffer(&BufferDescriptor {
            label: None,
            size: size_of::<ShaderUniform>() as u64,
            usage: BufferUsages::UNIFORM | BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let machine_draw_sampler = device.create_sampler(&SamplerDescriptor {
            mag_filter: FilterMode::Nearest,
            min_filter: FilterMode::Linear,
            ..Default::default()
        });

        let renderer = Renderer::new(
            &device,
            egui_texture_view_format,
            RendererOptions {
                msaa_samples: 0,
                depth_stencil_format: None,
                dithering: true,
                predictable_texture_filtering: false,
            },
        );

        ConfigurationDependentData {
            adapter,
            device,
            queue,
            bind_group_layout,
            pipeline,
            uniform_buffer,
            machine_draw_sampler,
            renderer,
            surface,
        }
    }
}

trait WebgpuCompatibleDisplayContext: DisplayContext {
    fn produce_instance_and_surface(
        &self,
    ) -> Result<(Instance, Surface<'static>), CreateSurfaceError>;
}
