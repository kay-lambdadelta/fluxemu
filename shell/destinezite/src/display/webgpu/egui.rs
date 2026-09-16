use egui_wgpu::ScreenDescriptor;
use fluxemu_frontend::graphics::GraphicsRuntime;
use fluxemu_frontend_egui::rendering::{DrawTarget, EguiCapableGraphicsRuntime};
use fluxemu_graphics::api::GraphicsApi;
use nalgebra::Vector2;
use palette::Srgb;
use wgpu::{
    BindGroupDescriptor, BindGroupEntry, BindingResource, CommandEncoderDescriptor,
    CurrentSurfaceTexture, DownlevelCapabilities, DownlevelFlags, LoadOp, Operations,
    RenderPassColorAttachment, RenderPassDescriptor, StoreOp, TextureFormat, TextureViewDescriptor,
};

use crate::display::webgpu::{
    ConfigurationDependentData, WebgpuCompatibleDisplayContext, WebgpuGraphicsRuntime,
    shader::ShaderUniform,
};

impl<D: WebgpuCompatibleDisplayContext> EguiCapableGraphicsRuntime for WebgpuGraphicsRuntime<D> {
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
                        DrawTarget::Gui {
                            context,
                            mut full_output,
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

                            for (new_texture_id, image_delta) in full_output
                                .textures_delta
                                .set
                                .drain()
                                .flat_map(|(id, deltas)| {
                                    deltas.into_iter().map(move |delta| (id, delta))
                                })
                            {
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

                            for remove_texture_id in full_output.textures_delta.free.iter() {
                                tracing::trace!("Freeing egui texture {:?}", remove_texture_id);
                                renderer.free_texture(remove_texture_id);
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
                                runtime_guard.component_registry().interact_dyn(
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
                queue.submit([command_buffer]);

                self.display_handle.pre_present_notify();
                queue.present(surface_texture);
            }
            _ => {
                self.refresh_surface();
            }
        }
    }
}

pub fn find_egui_texture_view_format(
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
