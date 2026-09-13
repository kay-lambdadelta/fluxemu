use std::sync::Arc;

use egui::{Context, FullOutput};
use fluxemu_graphics::api::{
    GraphicsApi,
    software::{
        Software,
        texture::{AsViewTextureMut, CopyMode},
    },
};
use fluxemu_runtime::{graphics::GraphicsRequirements, machine::Machine};
use nalgebra::{Point2, Vector2};
use palette::{Srgb, Srgba};

#[allow(clippy::large_enum_variant)]
pub enum DrawTarget<'a> {
    Egui {
        context: &'a Context,
        full_output: FullOutput,
    },
    Machine {
        machine: &'a Arc<Machine>,
    },
}

/// Extension trait for graphics apis
pub trait GraphicsRuntime: Sized + 'static {
    type GraphicsApi: GraphicsApi;

    fn reconfigure(&mut self, graphics_requirements: GraphicsRequirements<Self::GraphicsApi>);

    /// Refresh the surface
    fn refresh_surface(&mut self);

    /// Draw these items in this order
    fn present<'a>(
        &'a mut self,
        clear_color: Srgb<u8>,
        targets: impl IntoIterator<Item = DrawTarget<'a>>,
    );

    /// Graphics data components require
    fn component_initialization_data(
        &self,
    ) -> <Self::GraphicsApi as GraphicsApi>::InitializationData;

    /// Max texture size supported by this graphics backend
    fn max_texture_side(&self) -> u32;
}

#[inline]
pub fn present_machine_software<P: From<Srgba<u8>>>(
    machine: &Arc<Machine>,
    mut surface_buffer: impl AsViewTextureMut<P>,
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
        runtime_guard.component_registry().interact_dyn(
            framebuffer_parent_path,
            &runtime_guard.safe_advance_timestamp(),
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
                    .map_from(framebuffer_texture, CopyMode::Nearest, From::from);
            },
        );
    }
}
