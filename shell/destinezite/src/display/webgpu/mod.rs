use egui_wgpu::{Renderer, RendererOptions, ScreenDescriptor};
use fluxemu_frontend::graphics::{DrawTarget, GraphicsRuntime};
use fluxemu_graphics::api::{
    GraphicsApi,
    webgpu::{InitializationData, Webgpu},
};
use fluxemu_runtime::graphics::GraphicsRequirements;
use nalgebra::Vector2;
use palette::Srgb;
use pollster::FutureExt;
use wgpu::{
    Adapter, BindGroupDescriptor, BindGroupEntry, BindGroupLayout, BindGroupLayoutDescriptor,
    BindGroupLayoutEntry, BindingResource, BindingType, BlendState, Buffer, BufferBindingType,
    BufferDescriptor, BufferUsages, ColorTargetState, ColorWrites, CommandEncoderDescriptor,
    CreateSurfaceError, CurrentSurfaceTexture, Device, DeviceDescriptor, DownlevelCapabilities,
    DownlevelFlags, ExperimentalFeatures, FilterMode, FragmentState, Instance, LoadOp, MemoryHints,
    MultisampleState, Operations, PipelineCompilationOptions, PipelineLayoutDescriptor, PollType,
    PrimitiveState, Queue, RenderPassColorAttachment, RenderPassDescriptor, RenderPipeline,
    RenderPipelineDescriptor, Sampler, SamplerBindingType, SamplerDescriptor,
    ShaderModuleDescriptor, ShaderSource, ShaderStages, StoreOp, Surface, TextureFormat,
    TextureSampleType, TextureViewDescriptor, TextureViewDimension, Trace, VertexState,
    util::initialize_adapter_from_env_or_default,
};

use crate::display::{
    RuntimeAssociatedDisplayContext,
    webgpu::shader::{NORMAL_SHADER, ShaderUniform},
};

#[cfg(feature = "windowing")]
mod windowing;

#[cfg(feature = "drm")]
mod drm;

mod shader;

pub struct WebgpuGraphicsRuntime<H> {
    display_handle: H,
    configuration_dependent_data: Option<ConfigurationDependentData>,
}

impl<H: WebgpuCompatibleDisplayContext> GraphicsRuntime for WebgpuGraphicsRuntime<H> {
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

    fn present<'a>(
        &'a mut self,
        clear_color: Srgb<u8>,
        targets: impl IntoIterator<Item = DrawTarget<'a>>,
    ) {
        let ConfigurationDependentData {
            adapter,
            device,
            queue,
            bind_group_layout,
            pipeline,
            uniform_buffer,
            machine_draw_sampler,
            renderer,
            surface,
            ..
        } = self.configuration_dependent_data.as_mut().unwrap();

        let clear_color: Srgb<f64> = clear_color.into();
        let surface_config = surface.get_configuration().unwrap();

        match surface.get_current_texture() {
            CurrentSurfaceTexture::Success(surface_texture) => {
                let surface_texture_size = surface_texture.texture.size();

                let mut encoder =
                    device.create_command_encoder(&CommandEncoderDescriptor { label: None });

                let egui_texture_view_format = find_egui_texture_view_format(
                    surface_config.format,
                    adapter.get_downlevel_capabilities(),
                );

                for target in targets.into_iter() {
                    match target {
                        DrawTarget::Egui {
                            context,
                            full_output,
                        } => {
                            let surface_texture_view =
                                surface_texture.texture.create_view(&TextureViewDescriptor {
                                    format: Some(egui_texture_view_format),
                                    ..Default::default()
                                });

                            let render_pass_descriptor = RenderPassDescriptor {
                                label: None,
                                color_attachments: &[Some(RenderPassColorAttachment {
                                    view: &surface_texture_view,
                                    resolve_target: None,
                                    ops: Operations {
                                        load: LoadOp::Clear(wgpu::Color {
                                            r: clear_color.red,
                                            g: clear_color.green,
                                            b: clear_color.blue,
                                            a: 1.0,
                                        }),
                                        store: StoreOp::Store,
                                    },
                                    depth_slice: None,
                                })],
                                depth_stencil_attachment: None,
                                timestamp_writes: None,
                                occlusion_query_set: None,
                                multiview_mask: None,
                            };

                            let primitives = context
                                .tessellate(full_output.shapes, full_output.pixels_per_point);

                            for (new_texture_id, image_delta) in full_output.textures_delta.set {
                                renderer.update_texture(
                                    device,
                                    queue,
                                    new_texture_id,
                                    &image_delta,
                                );
                            }

                            let screen_descriptor = ScreenDescriptor {
                                size_in_pixels: [
                                    surface_texture_size.width,
                                    surface_texture_size.height,
                                ],
                                pixels_per_point: full_output.pixels_per_point,
                            };

                            renderer.update_buffers(
                                device,
                                queue,
                                &mut encoder,
                                &primitives,
                                &screen_descriptor,
                            );

                            let render_pass = encoder.begin_render_pass(&render_pass_descriptor);

                            renderer.render(
                                &mut render_pass.forget_lifetime(),
                                &primitives,
                                &screen_descriptor,
                            );

                            for remove_texture_id in full_output.textures_delta.free {
                                tracing::trace!("Freeing egui texture {:?}", remove_texture_id);
                                renderer.free_texture(&remove_texture_id);
                            }
                        }
                        DrawTarget::Machine { machine } => {
                            let surface_texture_view = surface_texture
                                .texture
                                .create_view(&TextureViewDescriptor::default());

                            let render_pass_descriptor = RenderPassDescriptor {
                                label: None,
                                color_attachments: &[Some(RenderPassColorAttachment {
                                    view: &surface_texture_view,
                                    resolve_target: None,
                                    ops: Operations {
                                        load: LoadOp::Clear(wgpu::Color {
                                            r: clear_color.red,
                                            g: clear_color.green,
                                            b: clear_color.blue,
                                            a: 1.0,
                                        }),
                                        store: StoreOp::Store,
                                    },
                                    depth_slice: None,
                                })],
                                depth_stencil_attachment: None,
                                timestamp_writes: None,
                                occlusion_query_set: None,
                                multiview_mask: None,
                            };

                            let mut render_pass =
                                encoder.begin_render_pass(&render_pass_descriptor);

                            render_pass.set_pipeline(pipeline);

                            // We lock the guards until the operation is done to stop race conditions
                            let runtime_guard = machine.enter_runtime();
                            let framebuffer_paths = runtime_guard.framebuffer_paths();

                            for framebuffer_path in framebuffer_paths.iter() {
                                let framebuffer_parent_path = framebuffer_path.parent().unwrap();

                                // Ensure we are at least on this frame for this component
                                runtime_guard.registry().interact_dyn(
                                    framebuffer_parent_path,
                                    &runtime_guard.safe_advance_timestamp(),
                                    |component| {
                                        let framebuffer = component.get_framebuffer(framebuffer_path.name());

                                        let framebuffer_texture: &<Self::GraphicsApi as GraphicsApi>::Framebuffer =
                                            framebuffer.downcast_ref().unwrap();

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

                                        queue
                                            .write_buffer(uniform_buffer, 0, bytemuck::bytes_of(&uniforms));

                                        let bind_group =  device.create_bind_group(&BindGroupDescriptor {
                                            label: None,
                                            layout: bind_group_layout,
                                            entries: &[
                                                BindGroupEntry {
                                                    binding: 0,
                                                    resource: uniform_buffer.as_entire_binding(),
                                                },
                                                BindGroupEntry {
                                                    binding: 1,
                                                    resource: BindingResource::TextureView(&texture_view),
                                                },
                                                BindGroupEntry {
                                                    binding: 2,
                                                    resource: BindingResource::Sampler(machine_draw_sampler),
                                                },
                                            ],
                                        });

                                        render_pass.set_bind_group(0, &bind_group, &[]);
                                        render_pass.draw(0..3, 0..1);
                                    },
                                );
                            }
                        }
                    }
                }

                let command_buffer = encoder.finish();
                let submission_index = queue.submit([command_buffer]);

                self.display_handle.pre_present_notify();
                surface_texture.present();

                device
                    .poll(PollType::Wait {
                        submission_index: Some(submission_index),
                        timeout: None,
                    })
                    .unwrap();
            }
            _ => {
                self.refresh_surface();
            }
        }
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

fn find_egui_texture_view_format(
    surface_format: TextureFormat,
    downlevel_capabilities: DownlevelCapabilities,
) -> TextureFormat {
    if downlevel_capabilities
        .flags
        .contains(DownlevelFlags::SURFACE_VIEW_FORMATS)
    {
        surface_format.remove_srgb_suffix()
    } else {
        surface_format
    }
}

trait WebgpuCompatibleDisplayContext:
    RuntimeAssociatedDisplayContext<WebgpuGraphicsRuntime<Self>>
{
    fn produce_instance_and_surface(
        &self,
    ) -> Result<(Instance, Surface<'static>), CreateSurfaceError>;
}
