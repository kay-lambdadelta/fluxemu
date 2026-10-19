use fluxemu_frontend::graphics::GraphicsRuntime;
use fluxemu_graphics::{
    api::{GraphicsApi, software::Software},
    texture::{CowTexture, OwnedTexture},
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

    fn screenshot(&self) -> CowTexture<'_, Srgba<u8>> {
        OwnedTexture::new(0, 0).into()
    }
}
