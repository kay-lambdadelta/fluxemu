use fluxemu_egui_software_renderer::{Renderer, callback::Callback};
use fluxemu_frontend_egui::rendering::{EguiCapableGraphicsRuntime, software::FramebufferCallback};
use fluxemu_graphics::{
    api::{GraphicsApi, software::Software},
    texture::{AsViewTexture, CopyMode, CowTexture, OwnedTexture},
};
use fluxemu_runtime::graphics::GraphicsRequirements;
use palette::{Srgb, Srgba, cast::Packed, named::BLACK, rgb::channels::Bgra};

pub struct GraphicsRuntime {
    egui_renderer: Renderer,
    pub texture: OwnedTexture<Srgba<u8>>,
}

impl Default for GraphicsRuntime {
    fn default() -> Self {
        Self {
            egui_renderer: Renderer::default(),
            texture: OwnedTexture::new(256, 256),
        }
    }
}

impl fluxemu_frontend::graphics::GraphicsRuntime for GraphicsRuntime {
    type GraphicsApi = Software;

    fn reconfigure(&mut self, _graphics_requirements: GraphicsRequirements<Self::GraphicsApi>) {}

    fn refresh_surface(&mut self) {}

    fn component_initialization_data(
        &self,
    ) -> <Self::GraphicsApi as GraphicsApi>::InitializationData {
    }

    fn max_texture_side(&self) -> u32 {
        u32::MAX
    }

    fn screenshot(&self) -> CowTexture<'_, Srgba<u8>> {
        self.texture.as_view().into()
    }
}

impl EguiCapableGraphicsRuntime for GraphicsRuntime {
    fn present(
        &mut self,
        context: &egui::Context,
        clear_color: Srgb<u8>,
        full_output: egui::FullOutput,
    ) {
        self.texture.fill(clear_color.into());

        self.egui_renderer
            .render::<_, 2>(context, full_output, &mut self.texture);
    }

    fn produce_callback_for_framebuffer(
        &mut self,
        rect: egui::Rect,
        framebuffer: &<Self::GraphicsApi as GraphicsApi>::Framebuffer,
    ) -> egui::PaintCallback {
        let mut converted_framebuffer =
            OwnedTexture::from_value(framebuffer.width(), framebuffer.height(), BLACK.into());
        converted_framebuffer.map_from(framebuffer, CopyMode::Nearest, From::from);

        let callback = FramebufferCallback::new(converted_framebuffer);

        Callback::<Packed<Bgra, [u8; 4]>>::new_paint_callback(rect, callback)
    }
}
