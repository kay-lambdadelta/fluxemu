use fluxemu_egui_software_renderer::Renderer;
use fluxemu_frontend::graphics::{DisplayContext, GraphicsRuntime};
use fluxemu_graphics::{
    api::{GraphicsApi, software::Software},
    texture::{AsViewTextureMut, CowTexture},
};
use fluxemu_runtime::graphics::GraphicsRequirements;
use palette::{Srgba, cast::Packed, rgb::channels::Bgra};

#[cfg(feature = "windowing")]
mod windowing;

#[cfg(feature = "drm")]
mod drm;

#[cfg(feature = "egui")]
mod egui;

pub struct SoftwareGraphicsRuntime<D: SoftwareCompatibleDisplayContext> {
    renderer: Renderer,
    surface: D::Surface,
    display_handle: D,
}

impl<D: SoftwareCompatibleDisplayContext> GraphicsRuntime for SoftwareGraphicsRuntime<D> {
    type GraphicsApi = Software;

    fn reconfigure(&mut self, _graphics_requirements: GraphicsRequirements<Self::GraphicsApi>) {
        // Nothing. Software backend is completely static
    }

    fn refresh_surface(&mut self) {
        self.display_handle
            .resize_surface(&mut self.surface)
            .unwrap();
    }

    fn component_initialization_data(
        &self,
    ) -> <Self::GraphicsApi as GraphicsApi>::InitializationData {
    }

    fn max_texture_side(&self) -> u32 {
        u32::MAX
    }

    fn screenshot(&self) -> CowTexture<'_, Srgba<u8>> {
        todo!()
    }
}

pub trait SoftwareCompatibleDisplayContext: DisplayContext {
    type Surface;
    type ResizeError: std::error::Error;
    type MappingError: std::error::Error;
    type PresentError: std::error::Error;

    fn resize_surface(&self, surface: &mut Self::Surface) -> Result<(), Self::ResizeError>;

    fn map_surface_buffer<'a>(
        &'a self,
        surface: &'a mut Self::Surface,
    ) -> Result<impl AsViewTextureMut<Packed<Bgra, [u8; 4]>> + 'a, Self::MappingError>;

    fn present(&self, surface: &mut Self::Surface) -> Result<(), Self::PresentError>;
}
