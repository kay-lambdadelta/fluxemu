use egui::{Context, FullOutput, PaintCallback};
use fluxemu_frontend::graphics::GraphicsRuntime;
use fluxemu_graphics::api::GraphicsApi;
use palette::Srgb;

pub mod software;
pub mod webgpu;

pub trait EguiCapableGraphicsRuntime: GraphicsRuntime {
    fn present(&mut self, context: &Context, clear_color: Srgb<u8>, full_output: FullOutput);

    fn produce_callback_for_framebuffer(
        &mut self,
        rect: egui::Rect,
        framebuffer: &<Self::GraphicsApi as GraphicsApi>::Framebuffer,
    ) -> PaintCallback;
}
