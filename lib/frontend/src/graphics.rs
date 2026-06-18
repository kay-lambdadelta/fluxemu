use std::sync::Arc;

use egui::{Context, FullOutput};
use fluxemu_graphics::api::GraphicsApi;
use fluxemu_runtime::{graphics::GraphicsRequirements, machine::Machine};
use palette::Srgb;

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
