use fluxemu_egui_software_renderer::{Renderer, callback::Callback};
use fluxemu_frontend_egui::rendering::{EguiCapableGraphicsRuntime, software::FramebufferCallback};
use fluxemu_graphics::{
    api::{GraphicsApi, software::Software},
    shader::ShaderType,
    texture::{CopyMode, CowTexture, OwnedTexture},
};
use fluxemu_runtime::graphics::GraphicsRequirements;
use palette::{Srgb, Srgba, named::BLACK};

use crate::runtime::framebuffer::{Framebuffer, FramebufferError, Pixel};

pub struct GraphicsRuntime {
    egui_renderer: Renderer,
    pub framebuffer: Framebuffer,
}

impl GraphicsRuntime {
    pub fn new() -> Result<Self, FramebufferError> {
        Ok(Self {
            egui_renderer: Renderer::default(),
            framebuffer: Framebuffer::open()?,
        })
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
        let texture = self.framebuffer.front_page();

        let mut screenshot =
            OwnedTexture::from_value(texture.width(), texture.height(), BLACK.into());
        screenshot.map_from(texture, CopyMode::Nearest, From::from);

        screenshot.into()
    }
}

impl EguiCapableGraphicsRuntime for GraphicsRuntime {
    fn present(
        &mut self,
        context: &egui::Context,
        clear_color: Srgb<u8>,
        full_output: egui::FullOutput,
    ) {
        self.framebuffer.wait_for_back_page().unwrap();

        let page = self.framebuffer.back_page();

        page.fill(clear_color.into());

        self.egui_renderer
            .render::<_, 2>(context, full_output, page);

        self.framebuffer.present().unwrap();
    }

    fn produce_callback_for_framebuffer(
        &mut self,
        rect: egui::Rect,
        framebuffer: &<Self::GraphicsApi as GraphicsApi>::Framebuffer,
        _shader: ShaderType,
    ) -> egui::PaintCallback {
        let mut converted_framebuffer =
            OwnedTexture::from_value(framebuffer.width(), framebuffer.height(), BLACK.into());
        converted_framebuffer.map_from(framebuffer, CopyMode::Nearest, From::from);

        let callback = FramebufferCallback::new(converted_framebuffer);

        Callback::<Pixel>::new_paint_callback(rect, callback)
    }
}
