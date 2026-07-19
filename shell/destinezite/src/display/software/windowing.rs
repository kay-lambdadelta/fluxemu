use std::sync::Arc;

use fluxemu_egui_software_renderer::Renderer;
use fluxemu_graphics::api::software::{
    Software,
    texture::{AsViewTexture, AsViewTextureMut, RefMutTexture, RefTexture},
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

impl RuntimeAssociatedDisplayContext<SoftwareGraphicsRuntime<Self>> for Arc<Window> {
    fn produce_runtime(
        &self,
        _graphics_requirements: GraphicsRequirements<Software>,
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
    ) -> Result<impl AsViewTextureMut<Packed<Bgra, [u8; 4]>> + 'a, Self::MappingError> {
        let buffer = surface.buffer_mut()?;

        Ok(SurfaceBufferGuard { buffer })
    }

    fn present(&self, surface: &mut Self::Surface) -> Result<(), Self::PresentError> {
        let buffer = surface.buffer_mut()?;

        buffer.present()?;

        Ok(())
    }
}
