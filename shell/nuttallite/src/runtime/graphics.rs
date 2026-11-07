use fluxemu_egui_software_renderer::{Renderer, callback::Callback};
use fluxemu_frontend_egui::rendering::{EguiCapableGraphicsRuntime, software::FramebufferCallback};
use fluxemu_graphics::{
    api::{GraphicsApi, software::Software},
    shader::ShaderType,
    texture::{AsViewTexture, CopyMode, CowTexture, OwnedTexture},
};
use fluxemu_math::rectangle::Rectangle;
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

    fn screenshot<'a>(
        &self,
        framebuffer: &'a <Self::GraphicsApi as GraphicsApi>::Framebuffer,
    ) -> Option<CowTexture<'a, Srgba<u8>>> {
        Some(framebuffer.as_view().into())
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
        display_rectangle: Rectangle<usize>,
        _shader: ShaderType,
    ) -> egui::PaintCallback {
        let framebuffer =
            framebuffer.view(display_rectangle.x_range(), display_rectangle.y_range());

        let mut converted_framebuffer =
            OwnedTexture::from_value(framebuffer.width(), framebuffer.height(), BLACK.into());
        converted_framebuffer.map_from(framebuffer, CopyMode::Nearest, From::from);

        let callback = FramebufferCallback::new(converted_framebuffer);

        Callback::<Pixel>::new_paint_callback(rect, callback)
    }
}
