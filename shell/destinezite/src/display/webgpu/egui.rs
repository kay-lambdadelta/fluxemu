use egui::{Context, FullOutput, PaintCallback};
use egui_wgpu::{Callback, ScreenDescriptor};
use fluxemu_frontend::graphics::GraphicsRuntime;
use fluxemu_frontend_egui::rendering::{EguiCapableGraphicsRuntime, webgpu::FramebufferCallback};
use fluxemu_graphics::api::GraphicsApi;
use palette::Srgb;
use wgpu::{
    CommandEncoderDescriptor, CurrentSurfaceTexture, LoadOp, Operations, RenderPassColorAttachment,
    RenderPassDescriptor, StoreOp, TextureViewDescriptor,
};

use crate::display::webgpu::{
    ConfigurationDependentData, WebgpuCompatibleDisplayContext, WebgpuGraphicsRuntime,
};

impl<D: WebgpuCompatibleDisplayContext> EguiCapableGraphicsRuntime for WebgpuGraphicsRuntime<D> {
    fn present(&mut self, context: &Context, clear_color: Srgb<u8>, mut full_output: FullOutput) {
        let ConfigurationDependentData {
            device,
            queue,
            renderer,
            surface,
            ..
        } = self.configuration_dependent_data.as_mut().unwrap();

        let clear_color: Srgb<f64> = clear_color.into();

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

                // Serialize on this submission
                {
                    let _guard = self.gpu_submission_lock.lock().unwrap();
                    queue.submit([command_buffer]);
                }

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
    ) -> PaintCallback {
        let ConfigurationDependentData {
            framebuffer_callback_resources,
            ..
        } = self.configuration_dependent_data.as_mut().unwrap();

        let framebuffer = framebuffer.clone();

        Callback::new_paint_callback(
            rect,
            FramebufferCallback::new(framebuffer, framebuffer_callback_resources.clone()),
        )
    }
}
