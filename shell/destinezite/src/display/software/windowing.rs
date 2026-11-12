use fluxemu_frontend::graphics::ProducableGraphicsRuntime;
use fluxemu_frontend_egui::rendering::software::{
    SoftwareCompatibleDisplayContext, SoftwareGraphicsRuntime,
};
use fluxemu_graphics::{
    api::software::Software,
    texture::{AsViewTexture, AsViewTextureMut, RefMutTexture, RefTexture},
};
use fluxemu_runtime::graphics::GraphicsRequirements;
use palette::{cast::Packed, rgb::channels::Bgra};
use softbuffer::{Buffer, Context, SoftBufferError, Surface};

use crate::event_loop::windowing::Window;

pub struct SurfaceBufferGuard<'a> {
    buffer: Buffer<'a, Window, Window>,
}

impl AsViewTexture<Packed<Bgra, [u8; 4]>> for SurfaceBufferGuard<'_> {
    fn as_view(&self) -> RefTexture<'_, Packed<Bgra, [u8; 4]>> {
        let width = self.buffer.width().get() as usize;
        let height = self.buffer.height().get() as usize;

        RefTexture::from_storage(width, height, bytemuck::cast_slice(&self.buffer))
    }
}

impl AsViewTextureMut<Packed<Bgra, [u8; 4]>> for SurfaceBufferGuard<'_> {
    fn as_view_mut(&mut self) -> RefMutTexture<'_, Packed<Bgra, [u8; 4]>> {
        let width = self.buffer.width().get() as usize;
        let height = self.buffer.height().get() as usize;

        RefMutTexture::from_storage(width, height, bytemuck::cast_slice_mut(&mut self.buffer))
    }
}

impl ProducableGraphicsRuntime<Window> for SoftwareGraphicsRuntime<Window> {
    fn new(window: &Window, _graphics_requirements: GraphicsRequirements<Software>) -> Self {
        let context = Context::new(window.clone()).unwrap();
        let mut surface = Surface::new(&context, window.clone()).unwrap();

        let window_dimensions = window.0.inner_size();

        surface
            .resize(
                window_dimensions.width.try_into().unwrap(),
                window_dimensions.height.try_into().unwrap(),
            )
            .unwrap();

        SoftwareGraphicsRuntime::new(window.clone(), surface)
    }

    fn display_context(&self) -> &Window {
        SoftwareGraphicsRuntime::display_context(self)
    }
}

impl SoftwareCompatibleDisplayContext for Window {
    type MappingError = SoftBufferError;
    type Pixel = Packed<Bgra, [u8; 4]>;
    type PresentError = SoftBufferError;
    type ResizeError = SoftBufferError;
    type Surface = softbuffer::Surface<Window, Window>;

    fn resize_surface(&self, surface: &mut Self::Surface) -> Result<(), Self::ResizeError> {
        let window_dimensions = self.0.inner_size();

        surface.resize(
            window_dimensions.width.try_into().unwrap(),
            window_dimensions.height.try_into().unwrap(),
        )
    }

    fn map_surface_buffer<'a>(
        &'a self,
        surface: &'a mut Self::Surface,
    ) -> Result<impl AsViewTextureMut<Self::Pixel> + 'a, Self::MappingError> {
        let buffer = surface.buffer_mut()?;

        Ok(SurfaceBufferGuard { buffer })
    }

    fn present(&self, surface: &mut Self::Surface) -> Result<(), Self::PresentError> {
        let buffer = surface.buffer_mut()?;

        buffer.present()?;

        Ok(())
    }
}
