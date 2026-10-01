use egui::PaintCallback;
use fluxemu_egui_software_renderer::callback::Callback;
use fluxemu_frontend_egui::rendering::{EguiCapableGraphicsRuntime, software::FramebufferCallback};
use fluxemu_graphics::{
    api::GraphicsApi,
    texture::{AsViewTextureMut, CopyMode, OwnedTexture},
};
use palette::{Srgb, cast::Packed, named::BLACK, rgb::channels::Bgra};

use crate::display::software::{SoftwareCompatibleDisplayContext, SoftwareGraphicsRuntime};

impl<D: SoftwareCompatibleDisplayContext> EguiCapableGraphicsRuntime
    for SoftwareGraphicsRuntime<D>
{
    fn present(
        &mut self,
        context: &egui::Context,
        clear_color: Srgb<u8>,
        full_output: egui::FullOutput,
    ) {
        let mut surface_buffer_guard = self
            .display_handle
            .map_surface_buffer(&mut self.surface)
            .unwrap();

        let mut surface_buffer = surface_buffer_guard.as_view_mut();

        surface_buffer.fill(clear_color.into());

        // Benchmarks say that a batch size of 16 is the most ideal across several low and mid power machines
        //
        // As far as throughput for realistic ui goes at the very least
        //
        // This is suggested by benchmarks on a i5-1245U and a RK3566T
        self.renderer
            .render::<_, 16>(context, full_output, &mut surface_buffer);

        drop(surface_buffer_guard);

        self.display_handle.pre_present_notify();
        self.display_handle.present(&mut self.surface).unwrap();
    }

    fn produce_callback_for_framebuffer(
        &mut self,
        rect: egui::Rect,
        framebuffer: &<Self::GraphicsApi as GraphicsApi>::Framebuffer,
    ) -> PaintCallback {
        let mut converted_framebuffer = OwnedTexture::<Packed<Bgra, [u8; 4]>>::from_value(
            framebuffer.width(),
            framebuffer.height(),
            BLACK.into(),
        );
        converted_framebuffer.map_from(framebuffer, CopyMode::Nearest, From::from);

        let callback = FramebufferCallback::new(converted_framebuffer);

        Callback::new_paint_callback(rect, callback)
    }
}
