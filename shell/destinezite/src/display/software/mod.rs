use std::sync::Arc;

use fluxemu_egui_software_renderer::Renderer;
use fluxemu_frontend::graphics::{DrawTarget, GraphicsRuntime};
use fluxemu_graphics::api::{
    GraphicsApi,
    software::{
        Software,
        texture::{AsViewTextureMut, CopyMode},
    },
};
use fluxemu_runtime::{graphics::GraphicsRequirements, machine::Machine};
use nalgebra::{Point2, Vector2};
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
                    present_machine(&mut surface_buffer, machine);
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

fn present_machine(
    mut surface_buffer: impl AsViewTextureMut<Packed<Bgra, [u8; 4]>>,
    machine: &Arc<Machine>,
) {
    let mut surface_buffer = surface_buffer.as_view_mut();
    let width = surface_buffer.width();
    let height = surface_buffer.height();

    let destination_dimensions: Vector2<f32> = Vector2::new(width, height).cast();

    let runtime_guard = machine.enter_runtime();
    let framebuffer_paths = runtime_guard.framebuffer_paths();

    for framebuffer_path in framebuffer_paths.iter() {
        let framebuffer_parent_path = framebuffer_path.parent().unwrap();

        // Ensure we are at least on this frame for this component
        runtime_guard.registry().interact_dyn(
            framebuffer_parent_path,
            runtime_guard.safe_advance_timestamp(),
            |component| {
                let framebuffer = component.get_framebuffer(framebuffer_path.name());

                let framebuffer_texture: &<Software as GraphicsApi>::Framebuffer =
                    framebuffer.downcast_ref().unwrap();

                let source_dimensions: Vector2<f32> = framebuffer_texture.size().cast();

                let source_aspect = source_dimensions.x / source_dimensions.y;
                let destination_aspect = destination_dimensions.x / destination_dimensions.y;

                let (scaled_dimensions, offset) = if source_aspect > destination_aspect {
                    let scaled_width = destination_dimensions.x;
                    let scaled_height = destination_dimensions.x / source_aspect;

                    let offset = Point2::new(
                        0,
                        ((destination_dimensions.y - scaled_height) / 2.0) as usize,
                    );

                    (Vector2::new(scaled_width, scaled_height), offset)
                } else {
                    let scaled_width = destination_dimensions.y * source_aspect;
                    let scaled_height = destination_dimensions.y;

                    let offset = Point2::new(
                        ((destination_dimensions.x - scaled_width) / 2.0) as usize,
                        0,
                    );

                    (Vector2::new(scaled_width, scaled_height), offset)
                };

                let min = offset;
                let max = offset + scaled_dimensions.try_cast().unwrap();

                surface_buffer
                    .view_mut(min.x..max.x, min.y..max.y)
                    .copy_from(framebuffer_texture, CopyMode::Nearest);
            },
        );
    }
}
