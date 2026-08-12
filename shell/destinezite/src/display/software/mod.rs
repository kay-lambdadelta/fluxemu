use fluxemu_egui_software_renderer::Renderer;
use fluxemu_frontend::graphics::{DrawTarget, GraphicsRuntime, present_machine_software};
use fluxemu_graphics::api::{
    GraphicsApi,
    software::{Software, texture::AsViewTextureMut},
};
use fluxemu_runtime::graphics::GraphicsRequirements;
use palette::{Srgb, cast::Packed, rgb::channels::Bgra};

use crate::display::RuntimeAssociatedDisplayContext;

#[cfg(feature = "windowing")]
mod windowing;

#[cfg(feature = "drm")]
mod drm;

pub struct SoftwareGraphicsRuntime<H: SoftwareCompatibleDisplayContext> {
    renderer: Renderer,
    surface: H::Surface,
    display_handle: H,
}

impl<H: SoftwareCompatibleDisplayContext> GraphicsRuntime for SoftwareGraphicsRuntime<H> {
    type GraphicsApi = Software;

    fn reconfigure(&mut self, _graphics_requirements: GraphicsRequirements<Self::GraphicsApi>) {
        // Nothing. Software backend is completely static
    }

    fn refresh_surface(&mut self) {
        self.display_handle
            .resize_surface(&mut self.surface)
            .unwrap();
    }

    fn present<'a>(
        &'a mut self,
        clear_color: Srgb<u8>,
        targets: impl IntoIterator<Item = DrawTarget<'a>>,
    ) {
        let mut surface_buffer_guard = self
            .display_handle
            .map_surface_buffer(&mut self.surface)
            .unwrap();
        let mut surface_buffer = surface_buffer_guard.as_view_mut();

        surface_buffer.fill(clear_color.into());

        for target in targets {
            match target {
                DrawTarget::Egui {
                    context,
                    full_output,
                } => {
                    // Benchmarks say that a batch size of 16 is the most ideal across several low and mid power machines
                    //
                    // As far as throughput for realistic ui goes at the very least
                    //
                    // This is suggested by benchmarks on a i5-1245U and a RK3566T
                    self.renderer
                        .render::<_, 16>(context, full_output, &mut surface_buffer);
                }
                DrawTarget::Machine { machine } => {
                    present_machine_software(machine, &mut surface_buffer);
                }
            }
        }

        drop(surface_buffer_guard);

        self.display_handle.pre_present_notify();
        self.display_handle.present(&mut self.surface).unwrap();
    }

    fn component_initialization_data(
        &self,
    ) -> <Self::GraphicsApi as GraphicsApi>::InitializationData {
    }

    fn max_texture_side(&self) -> u32 {
        u32::MAX
    }
}

pub trait SoftwareCompatibleDisplayContext:
    RuntimeAssociatedDisplayContext<SoftwareGraphicsRuntime<Self>>
{
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
