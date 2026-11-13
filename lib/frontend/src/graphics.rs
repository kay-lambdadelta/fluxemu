use fluxemu_graphics::{
    api::{GraphicsApi, software::Software},
    texture::{AsViewTexture, CowTexture, Storage, Texture},
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
    fn with_context<T>(
        &mut self,
        callback: impl FnOnce(&mut <Self::GraphicsApi as GraphicsApi>::Context) -> T,
    ) -> T;
    fn max_texture_side(&self) -> u32;
    fn screenshot<'a>(
        &self,
        framebuffer: &'a <Self::GraphicsApi as GraphicsApi>::Framebuffer,
    ) -> Option<CowTexture<'a, Srgba<u8>>>;
}

pub trait ProducableGraphicsRuntime<D: DisplayContext>: GraphicsRuntime {
    fn new(
        display_context: &D,
        graphics_requirements: GraphicsRequirements<Self::GraphicsApi>,
    ) -> Self;

    fn display_context(&self) -> &D;
}

#[derive(Debug, Default, Clone, Copy)]
pub struct PassthroughGraphicsRuntime;

impl GraphicsRuntime for PassthroughGraphicsRuntime {
    type GraphicsApi = Software;

    fn reconfigure(&mut self, _graphics_requirements: GraphicsRequirements<Self::GraphicsApi>) {}

    fn refresh_surface(&mut self) {}

    fn with_context<T>(&mut self, callback: impl FnOnce(&mut ()) -> T) -> T {
        callback(&mut ())
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
