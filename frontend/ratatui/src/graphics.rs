use fluxemu_frontend::graphics::GraphicsRuntime;
use fluxemu_graphics::{
    api::{GraphicsApi, software::Software},
    texture::{AsViewTexture, CowTexture},
};
use fluxemu_runtime::graphics::GraphicsRequirements;
use palette::Srgba;

#[derive(Debug, Default)]
pub struct TerminalGraphicsRuntime;

impl GraphicsRuntime for TerminalGraphicsRuntime {
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
