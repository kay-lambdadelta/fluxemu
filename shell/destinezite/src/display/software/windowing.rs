use std::sync::Arc;

use fluxemu_egui_software_renderer::Renderer;
use fluxemu_graphics::api::software::{
    Software,
    texture::{AsTextureView, AsTextureViewMut, TextureView, TextureViewMut},
};
use fluxemu_runtime::graphics::GraphicsRequirements;
use palette::{cast::Packed, rgb::channels::Bgra};
use softbuffer::{Buffer, Context, SoftBufferError, Surface};
use winit::window::Window;

use crate::display::{
    RuntimeAssociatedDisplayContext,
    software::{SoftwareCompatibleDisplayContext, SoftwareGraphicsRuntime},
};

pub struct SurfaceBufferGuard<'a> {
    buffer: Buffer<'a, Arc<Window>, Arc<Window>>,
}

impl AsTextureView<Packed<Bgra, [u8; 4]>> for SurfaceBufferGuard<'_> {
    fn as_texture_view(&self) -> TextureView<'_, Packed<Bgra, [u8; 4]>> {
        let width = self.buffer.width().get() as usize;
        let height = self.buffer.height().get() as usize;

        TextureView::from_slice(bytemuck::cast_slice(&self.buffer), width, height)
    }
}

impl AsTextureViewMut<Packed<Bgra, [u8; 4]>> for SurfaceBufferGuard<'_> {
    fn as_texture_view_mut(&mut self) -> TextureViewMut<'_, Packed<Bgra, [u8; 4]>> {
        let width = self.buffer.width().get() as usize;
        let height = self.buffer.height().get() as usize;

        TextureViewMut::from_slice(bytemuck::cast_slice_mut(&mut self.buffer), width, height)
    }
}

impl RuntimeAssociatedDisplayContext<SoftwareGraphicsRuntime<Self>> for Arc<Window> {
    type ProduceDataArgs<'a> = ();

    fn produce_runtime(
        &self,
        _graphics_requirements: GraphicsRequirements<Software>,
        _seat: (),
    ) -> SoftwareGraphicsRuntime<Self> {
        let context = Context::new(self.clone()).unwrap();
        let mut surface = Surface::new(&context, self.clone()).unwrap();

        let window_dimensions = self.inner_size();

        surface
            .resize(
                window_dimensions.width.try_into().unwrap(),
                window_dimensions.height.try_into().unwrap(),
            )
            .unwrap();

        SoftwareGraphicsRuntime {
            surface,
            renderer: Renderer::default(),
            display_handle: self.clone(),
        }
    }
}

impl SoftwareCompatibleDisplayContext for Arc<Window> {
    type MappingError = SoftBufferError;
    type PresentError = SoftBufferError;
    type ResizeError = SoftBufferError;
    type Surface = softbuffer::Surface<Arc<Window>, Arc<Window>>;
    type SurfaceBufferGuard<'a> = SurfaceBufferGuard<'a>;

    fn resize_surface(&self, surface: &mut Self::Surface) -> Result<(), Self::ResizeError> {
        let window_dimensions = self.inner_size();

        surface.resize(
            window_dimensions.width.try_into().unwrap(),
            window_dimensions.height.try_into().unwrap(),
        )
    }

    fn map_surface_buffer<'a>(
        &'a self,
        surface: &'a mut Self::Surface,
    ) -> Result<Self::SurfaceBufferGuard<'a>, Self::MappingError> {
        let buffer = surface.buffer_mut()?;

        Ok(SurfaceBufferGuard { buffer })
    }

    fn present(&self, surface: &mut Self::Surface) -> Result<(), Self::PresentError> {
        let buffer = surface.buffer_mut()?;

        buffer.present()?;

        Ok(())
    }
}
