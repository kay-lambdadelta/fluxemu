use std::sync::Arc;

use fluxemu_egui_software_renderer::Renderer;
use fluxemu_graphics::api::software::{Software, texture::TextureViewMut};
use fluxemu_runtime::graphics::GraphicsRequirements;
use palette::{cast::Packed, rgb::channels::Bgra};
use softbuffer::{Context, SoftBufferError, Surface};
use winit::window::Window;

use crate::display::{
    RuntimeAssociatedDisplayContext,
    software::{SoftwareCompatibleDisplayContext, SoftwareGraphicsRuntime},
};

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
    type ResizeError = SoftBufferError;
    type Surface = softbuffer::Surface<Arc<Window>, Arc<Window>>;

    fn resize_surface(&self, surface: &mut Self::Surface) -> Result<(), Self::ResizeError> {
        let window_dimensions = self.inner_size();

        surface.resize(
            window_dimensions.width.try_into().unwrap(),
            window_dimensions.height.try_into().unwrap(),
        )
    }

    fn map_surface_buffer(
        &self,
        surface: &mut Self::Surface,
        callback: impl FnOnce(TextureViewMut<'_, Packed<Bgra, [u8; 4]>>),
    ) -> Result<(), Self::MappingError> {
        let mut buffer = surface.buffer_mut()?;
        let width = buffer.width().get() as usize;
        let height = buffer.height().get() as usize;

        let texture_view =
            TextureViewMut::from_slice(bytemuck::cast_slice_mut(&mut buffer), width, height);

        callback(texture_view);

        buffer.present()?;

        Ok(())
    }
}
