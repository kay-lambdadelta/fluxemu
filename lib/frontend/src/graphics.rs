use fluxemu_graphics::{
    api::GraphicsApi,
    texture::{CowTexture, Storage, Texture},
};
use fluxemu_runtime::graphics::GraphicsRequirements;
use nalgebra::Vector2;
use palette::Srgba;

pub trait DisplayContext: 'static {
    fn dimensions(&self) -> Vector2<u32>;
    fn pre_present_notify(&mut self) {}
}

impl<STORAGE: Storage + 'static> DisplayContext for Texture<STORAGE> {
    fn dimensions(&self) -> Vector2<u32> {
        Texture::size(self).map(|s| s as u32)
    }
}

/// Extension trait for graphics apis
pub trait GraphicsRuntime: Sized + 'static {
    type GraphicsApi: GraphicsApi;

    fn reconfigure(&mut self, graphics_requirements: GraphicsRequirements<Self::GraphicsApi>);
    fn refresh_surface(&mut self);
    fn component_initialization_data(
        &self,
    ) -> <Self::GraphicsApi as GraphicsApi>::InitializationData;
    fn max_texture_side(&self) -> u32;
    fn screenshot(&self) -> CowTexture<'_, Srgba<u8>>;
}

pub trait ProducableGraphicsRuntime<D: DisplayContext>: GraphicsRuntime {
    fn new(
        display_context: &D,
        graphics_requirements: GraphicsRequirements<Self::GraphicsApi>,
    ) -> Self;

    fn display_context(&self) -> &D;
}
