use std::sync::Arc;

use egui::{Context, FullOutput, PaintCallback};
use egui_wgpu::{Callback, Renderer, RendererOptions, ScreenDescriptor};
use fluxemu_frontend::graphics::{DisplayContext, GraphicsRuntime, ProducableGraphicsRuntime};
use fluxemu_graphics::{
    api::{
        GraphicsApi,
        webgpu::{self, Webgpu, suggested_framebuffer_texture_format},
    },
    shader::ShaderType,
    texture::{CowTexture, OwnedTexture},
};
use fluxemu_math::rectangle::Rectangle;
use fluxemu_runtime::graphics::GraphicsRequirements;
use palette::{Srgb, Srgba};
use pollster::FutureExt;
use wgpu::{
    BufferDescriptor, BufferUsages, COPY_BYTES_PER_ROW_ALIGNMENT, CommandEncoderDescriptor,
    CreateSurfaceError, CurrentSurfaceTexture, DeviceDescriptor, ExperimentalFeatures, Instance,
    LoadOp, MapMode, MemoryHints, Operations, PollType, RenderPassColorAttachment,
    RenderPassDescriptor, StoreOp, Surface, TexelCopyBufferInfo, TexelCopyBufferLayout,
    TextureViewDescriptor, Trace, util::initialize_adapter_from_env_or_default,
};

use crate::rendering::{
    EguiCapableGraphicsRuntime,
    webgpu::{FramebufferCallback, Resources},
};

pub struct WebgpuGraphicsRuntime<D> {
    configuration_dependent_data: Option<ConfigurationDependentData>,
    display_handle: D,
}

impl<D: WebgpuCompatibleDisplayContext + Clone> ProducableGraphicsRuntime<D>
    for WebgpuGraphicsRuntime<D>
{
    fn new(display_context: &D, graphics_requirements: GraphicsRequirements<Webgpu>) -> Self {
        let configuration_dependent_data =
            ConfigurationDependentData::new(graphics_requirements, display_context);

        WebgpuGraphicsRuntime {
            display_handle: display_context.clone(),
            configuration_dependent_data: Some(configuration_dependent_data),
        }
    }

    fn display_context(&self) -> &D {
        &self.display_handle
    }
}

impl<D: WebgpuCompatibleDisplayContext> GraphicsRuntime for WebgpuGraphicsRuntime<D> {
    type GraphicsApi = Webgpu;

    fn reconfigure(&mut self, graphics_requirements: GraphicsRequirements<Self::GraphicsApi>) {
        // Drop old data
        drop(self.configuration_dependent_data.take().unwrap());

        self.configuration_dependent_data = Some(ConfigurationDependentData::new(
            graphics_requirements,
            &self.display_handle,
        ));
    }

    fn refresh_surface(&mut self) {
        let size = self.display_handle.dimensions();
        let ConfigurationDependentData {
            context: webgpu::Context { device, .. },
            surface,
            ..
        } = self.configuration_dependent_data.as_mut().unwrap();

        let mut surface_config = surface.get_configuration().unwrap();
        surface_config.width = size.x;
        surface_config.height = size.y;

        surface.configure(device, &surface_config);
    }

    fn with_context<T>(&mut self, callback: impl FnOnce(&mut webgpu::Context) -> T) -> T {
        callback(&mut self.configuration_dependent_data.as_mut().unwrap().context)
    }

    fn max_texture_side(&self) -> u32 {
        self.configuration_dependent_data
            .as_ref()
            .unwrap()
            .context
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

        let ConfigurationDependentData {
            context: webgpu::Context { device, queue },
            ..
        } = self.configuration_dependent_data.as_ref().unwrap();

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

        queue.submit([encoder.finish()]);

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
    context: webgpu::Context,
    renderer: Renderer,
    surface: Surface<'static>,
    framebuffer_callback_resources: Arc<Resources>,
}

impl ConfigurationDependentData {
    fn new(
        graphics_requirements: GraphicsRequirements<Webgpu>,
        display_handle: &impl WebgpuCompatibleDisplayContext,
    ) -> Self {
        let dimensions = display_handle.dimensions();
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
            context: webgpu::Context { device, queue },
            renderer,
            surface,
            framebuffer_callback_resources,
        }
    }
}

pub trait WebgpuCompatibleDisplayContext: DisplayContext {
    fn produce_instance_and_surface(
        &self,
    ) -> Result<(Instance, Surface<'static>), CreateSurfaceError>;
}

impl<D: WebgpuCompatibleDisplayContext> EguiCapableGraphicsRuntime for WebgpuGraphicsRuntime<D> {
    fn present(&mut self, context: &Context, clear_color: Srgb<u8>, mut full_output: FullOutput) {
        let ConfigurationDependentData {
            context: webgpu::Context { device, queue },
            renderer,
            surface,
            ..
        } = self.configuration_dependent_data.as_mut().unwrap();

        let clear_color: Srgb<_> = clear_color.into();

        match surface.get_current_texture() {
            CurrentSurfaceTexture::Success(surface_texture) => {
                let surface_texture_size = surface_texture.texture.size();

                let mut encoder =
                    device.create_command_encoder(&CommandEncoderDescriptor { label: None });

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

                let primitives =
                    context.tessellate(full_output.shapes, full_output.pixels_per_point);

                for (new_texture_id, image_delta) in full_output
                    .textures_delta
                    .set
                    .drain()
                    .flat_map(|(id, deltas)| deltas.into_iter().map(move |delta| (id, delta)))
                {
                    renderer.update_texture(device, queue, new_texture_id, &image_delta);
                }

                let screen_descriptor = ScreenDescriptor {
                    size_in_pixels: [surface_texture_size.width, surface_texture_size.height],
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

                for remove_texture_id in full_output.textures_delta.free.iter() {
                    tracing::trace!("Freeing egui texture {:?}", remove_texture_id);

                    renderer.free_texture(remove_texture_id);
                }

                let command_buffer = encoder.finish();

                queue.submit([command_buffer]);

                self.display_handle.pre_present_notify();
                queue.present(surface_texture);
            }
            _ => {
                self.refresh_surface();
            }
        }
    }

    fn produce_callback_for_framebuffer(
        &mut self,
        rect: egui::Rect,
        framebuffer: &<Self::GraphicsApi as GraphicsApi>::Framebuffer,
        display_rectangle: Rectangle<usize>,
        shader: ShaderType,
    ) -> PaintCallback {
        let ConfigurationDependentData {
            framebuffer_callback_resources,
            ..
        } = self.configuration_dependent_data.as_mut().unwrap();

        if framebuffer_callback_resources.shader() != shader {
            *framebuffer_callback_resources = framebuffer_callback_resources.with_shader(shader);
        }

        let framebuffer = framebuffer.clone();

        Callback::new_paint_callback(
            rect,
            FramebufferCallback::new(
                framebuffer,
                display_rectangle,
                framebuffer_callback_resources.clone(),
            ),
        )
    }
}
