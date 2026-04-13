use std::sync::Arc;

use bytemuck::{Pod, Zeroable};
use egui::{Context, FullOutput};
use egui_wgpu::{Renderer, RendererOptions, ScreenDescriptor};
use fluxemu_frontend::GraphicsRuntime;
use fluxemu_runtime::{
    graphics::{
        GraphicsApi, GraphicsRequirements,
        webgpu::{InitializationData, Requirements, Webgpu},
    },
    machine::Machine,
};
use nalgebra::Vector2;
use pollster::FutureExt;
use wgpu::{
    BindGroupDescriptor, BindGroupEntry, BindGroupLayout, BindGroupLayoutDescriptor,
    BindGroupLayoutEntry, BindingResource, BindingType, BlendState, Buffer, BufferBindingType,
    BufferDescriptor, BufferUsages, ColorTargetState, ColorWrites, CommandEncoderDescriptor,
    CompositeAlphaMode, CurrentSurfaceTexture, Device, DeviceDescriptor, DownlevelFlags,
    ExperimentalFeatures, FilterMode, FragmentState, Instance, InstanceDescriptor, LoadOp,
    MemoryHints, MultisampleState, Operations, PipelineCompilationOptions,
    PipelineLayoutDescriptor, PollType, PresentMode, PrimitiveState, Queue,
    RenderPassColorAttachment, RenderPassDescriptor, RenderPipeline, RenderPipelineDescriptor,
    Sampler, SamplerBindingType, SamplerDescriptor, ShaderModuleDescriptor, ShaderSource,
    ShaderStages, StoreOp, Surface, SurfaceConfiguration, TextureFormat, TextureSampleType,
    TextureUsages, TextureViewDescriptor, TextureViewDimension, Trace, VertexState,
    util::initialize_adapter_from_env_or_default,
};
use winit::window::Window;

use crate::windowing::WinitCompatibleGraphicsRuntime;

pub struct WebgpuGraphicsRuntime {
    renderer: Renderer,
    device: Device,
    queue: Queue,
    surface: Surface<'static>,
    window: Arc<Window>,
    pipeline: RenderPipeline,
    bind_group_layout: BindGroupLayout,
    uniform_buffer: Buffer,
    machine_draw_sampler: Sampler,
    surface_configuration: SurfaceConfiguration,
    egui_surface_format_view: Option<TextureFormat>,
}

impl GraphicsRuntime for WebgpuGraphicsRuntime {
    type GraphicsApi = Webgpu;

    fn present_egui_overlay(&mut self, context: &Context, full_output: FullOutput) {
        let primitives = context.tessellate(full_output.shapes, full_output.pixels_per_point);

        match self.surface.get_current_texture() {
            CurrentSurfaceTexture::Success(surface_texture) => {
                let surface_texture_size = surface_texture.texture.size();

                let surface_texture_view =
                    surface_texture.texture.create_view(&TextureViewDescriptor {
                        format: self.egui_surface_format_view,
                        ..Default::default()
                    });

                for (new_texture_id, image_delta) in full_output.textures_delta.set {
                    self.renderer.update_texture(
                        &self.device,
                        &self.queue,
                        new_texture_id,
                        &image_delta,
                    );
                }

                let mut encoder = self
                    .device
                    .create_command_encoder(&CommandEncoderDescriptor { label: None });

                let screen_descriptor = ScreenDescriptor {
                    size_in_pixels: [surface_texture_size.width, surface_texture_size.height],
                    pixels_per_point: full_output.pixels_per_point,
                };

                self.renderer.update_buffers(
                    &self.device,
                    &self.queue,
                    &mut encoder,
                    &primitives,
                    &screen_descriptor,
                );

                let render_pass = encoder.begin_render_pass(&RenderPassDescriptor {
                    label: None,
                    color_attachments: &[Some(RenderPassColorAttachment {
                        view: &surface_texture_view,
                        resolve_target: None,
                        ops: Operations {
                            load: LoadOp::Clear(wgpu::Color::BLACK),
                            store: StoreOp::Store,
                        },
                        depth_slice: None,
                    })],
                    depth_stencil_attachment: None,
                    timestamp_writes: None,
                    occlusion_query_set: None,
                    multiview_mask: None,
                });

                self.renderer.render(
                    &mut render_pass.forget_lifetime(),
                    &primitives,
                    &screen_descriptor,
                );

                let command_buffer = encoder.finish();
                let submission_index = self.queue.submit([command_buffer]);

                surface_texture.present();

                self.device
                    .poll(PollType::Wait {
                        submission_index: Some(submission_index),
                        timeout: None,
                    })
                    .unwrap();

                for remove_texture_id in full_output.textures_delta.free {
                    tracing::trace!("Freeing egui texture {:?}", remove_texture_id);
                    self.renderer.free_texture(&remove_texture_id);
                }
            }
            _ => {
                self.refresh_surface();
            }
        }
    }

    fn present_machine(&mut self, machine: &Arc<Machine>) {
        match self.surface.get_current_texture() {
            CurrentSurfaceTexture::Success(surface_texture) => {
                let surface_texture_size = surface_texture.texture.size();

                let surface_texture_view = surface_texture
                    .texture
                    .create_view(&TextureViewDescriptor::default());

                let mut encoder = self
                    .device
                    .create_command_encoder(&CommandEncoderDescriptor { label: None });

                let mut render_pass = encoder.begin_render_pass(&RenderPassDescriptor {
                    label: None,
                    color_attachments: &[Some(RenderPassColorAttachment {
                        view: &surface_texture_view,
                        resolve_target: None,
                        ops: Operations {
                            load: LoadOp::Clear(wgpu::Color::BLACK),
                            store: StoreOp::Store,
                        },
                        depth_slice: None,
                    })],
                    depth_stencil_attachment: None,
                    timestamp_writes: None,
                    occlusion_query_set: None,
                    multiview_mask: None,
                });

                render_pass.set_pipeline(&self.pipeline);

                // We lock the guards until the operation is done to stop race conditions
                let runtime_guard = machine.enter_runtime();

                let mut used_framebuffer_guards = Vec::default();
                let framebuffers = runtime_guard.framebuffers();

                for (display_path, framebuffer) in framebuffers.iter() {
                    // Ensure we are at least on this frame for this component
                    runtime_guard.registry().interact_dyn(
                        display_path.parent().unwrap(),
                        runtime_guard.safe_advance_timestamp(),
                        |_| {},
                    );

                    let framebuffer_guard = framebuffer.lock().unwrap();
                    let framebuffer_texture: &<Self::GraphicsApi as GraphicsApi>::Framebuffer =
                        framebuffer_guard.downcast_ref().unwrap();

                    let texture_view =
                        framebuffer_texture.create_view(&TextureViewDescriptor::default());
                    let size = framebuffer_texture.size();

                    let uniforms = ShaderUniform {
                        viewport_size: Vector2::new(
                            surface_texture_size.width as f32,
                            surface_texture_size.height as f32,
                        ),
                        framebuffer_size: Vector2::new(size.width as f32, size.height as f32),
                    };

                    self.queue
                        .write_buffer(&self.uniform_buffer, 0, bytemuck::bytes_of(&uniforms));

                    let bind_group = self.device.create_bind_group(&BindGroupDescriptor {
                        label: None,
                        layout: &self.bind_group_layout,
                        entries: &[
                            BindGroupEntry {
                                binding: 0,
                                resource: self.uniform_buffer.as_entire_binding(),
                            },
                            BindGroupEntry {
                                binding: 1,
                                resource: BindingResource::TextureView(&texture_view),
                            },
                            BindGroupEntry {
                                binding: 2,
                                resource: BindingResource::Sampler(&self.machine_draw_sampler),
                            },
                        ],
                    });

                    render_pass.set_bind_group(0, &bind_group, &[]);
                    render_pass.draw(0..3, 0..1);

                    used_framebuffer_guards.push(framebuffer_guard);
                }

                drop(render_pass);

                let command_buffer = encoder.finish();
                let submission_index = self.queue.submit([command_buffer]);

                surface_texture.present();

                self.device
                    .poll(PollType::Wait {
                        submission_index: Some(submission_index),
                        timeout: None,
                    })
                    .unwrap();

                // Allow those display components to continue again
                drop(used_framebuffer_guards);
            }
            _ => {
                self.refresh_surface();
            }
        }
    }

    fn refresh_surface(&mut self) {
        let size = self.window.inner_size();
        self.surface_configuration.width = size.width;
        self.surface_configuration.height = size.height;

        self.surface
            .configure(&self.device, &self.surface_configuration);
    }

    fn created_requirements(&self) -> <Self::GraphicsApi as GraphicsApi>::Requirements {
        Requirements {
            features: self.device.features(),
            limits: self.device.limits(),
        }
    }

    fn component_initialization_data(
        &self,
    ) -> <Self::GraphicsApi as GraphicsApi>::InitializationData {
        InitializationData {
            device: self.device.clone(),
            queue: self.queue.clone(),
        }
    }

    fn max_texture_side(&self) -> u32 {
        self.device.limits().max_texture_dimension_2d
    }
}

impl WinitCompatibleGraphicsRuntime for WebgpuGraphicsRuntime {
    fn new(window: Arc<Window>, requirements: GraphicsRequirements<Self::GraphicsApi>) -> Self {
        let window_size = window.inner_size();

        let instance = Instance::new(InstanceDescriptor::new_with_display_handle_from_env(
            Box::new(window.clone()),
        ));

        let surface = instance.create_surface(window.clone()).unwrap();

        let adapter = initialize_adapter_from_env_or_default(&instance, Some(&surface))
            .block_on()
            .expect("Creating adapter");

        let downlevel_capabilities = adapter.get_downlevel_capabilities();

        let capabilities = surface.get_capabilities(&adapter);

        let preferred_features = requirements.required.clone() | requirements.preferred.clone();

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
                required_features: requirements.required.features,
                required_limits: requirements.required.limits,
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

        let surface_format = capabilities
            .formats
            .iter()
            .copied()
            // Egui prefers non srgb textures
            .find(|format| format.is_srgb())
            .or_else(|| capabilities.formats.first().copied())
            .unwrap();

        let egui_surface_format_view = if downlevel_capabilities
            .flags
            .contains(DownlevelFlags::SURFACE_VIEW_FORMATS)
        {
            Some(surface_format.remove_srgb_suffix())
        } else {
            None
        };

        tracing::info!("Using surface texture format {:?}", surface_format);

        let surface_configuration = SurfaceConfiguration {
            format: surface_format,
            usage: TextureUsages::RENDER_ATTACHMENT,
            width: window_size.width,
            height: window_size.height,
            present_mode: PresentMode::AutoVsync,
            alpha_mode: CompositeAlphaMode::Opaque,
            view_formats: egui_surface_format_view.into_iter().collect(),
            desired_maximum_frame_latency: 2,
        };

        surface.configure(&device, &surface_configuration);

        let shader = device.create_shader_module(ShaderModuleDescriptor {
            label: None,
            source: ShaderSource::Wgsl(include_str!("shader/normal.wgsl").into()),
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
                    format: surface_format,
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
            egui_surface_format_view.unwrap_or(surface_format),
            RendererOptions {
                msaa_samples: 0,
                depth_stencil_format: None,
                dithering: true,
                predictable_texture_filtering: false,
            },
        );

        Self {
            renderer,
            device,
            queue,
            surface,
            window,
            bind_group_layout,
            uniform_buffer,
            pipeline,
            machine_draw_sampler,
            surface_configuration,
            egui_surface_format_view,
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct ShaderUniform {
    viewport_size: Vector2<f32>,
    framebuffer_size: Vector2<f32>,
}
