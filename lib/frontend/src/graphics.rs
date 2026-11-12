use std::sync::Arc;

use fluxemu_graphics::{
    api::{GraphicsApi, software::Software},
    texture::{AsViewTexture, CopyMode, CowTexture, OwnedTexture, Storage, Texture},
};
use fluxemu_runtime::{graphics::GraphicsRequirements, machine::Machine};
use nalgebra::Vector2;
use palette::{Srgba, named::BLACK};

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

pub fn capture_framebuffer(
    machine: &Arc<Machine>,
) -> Option<(OwnedTexture<Srgba<u8>>, Vector2<f32>)> {
    let runtime = machine.enter_runtime();
    let framebuffer_path = runtime.list_framebuffers().next()?;

    runtime.read_framebuffer::<Software, _>(
        framebuffer_path,
        runtime.safe_advance_timestamp(),
        |framebuffer, display_geometry| {
            let display_rectangle = display_geometry.resolve_display_rectangle(framebuffer);
            let view = framebuffer.view(display_rectangle.x_range(), display_rectangle.y_range());

            let mut captured = OwnedTexture::from_value(view.width(), view.height(), BLACK.into());
            captured.copy_from(view, CopyMode::Nearest);

            (captured, display_geometry.display_size(framebuffer))
        },
    )
}
