pub mod software;

use egui::{Context, FullOutput};
use fluxemu_runtime::{graphics::GraphicsApi, machine::Machine};

/// Extension trait for graphics apis
#[allow(async_fn_in_trait)]
pub trait GraphicsRuntime: Sized + 'static {
    type GraphicsApi: GraphicsApi;

    /// Refresh the surface
    fn refresh_surface(&mut self);

    fn present_egui_overlay(&mut self, context: &Context, full_output: FullOutput);
    fn present_machine(&mut self, machine: &Machine);

    /// Graphics data components require
    fn component_initialization_data(
        &self,
    ) -> <Self::GraphicsApi as GraphicsApi>::InitializationData;

    fn created_requirements(&self) -> <Self::GraphicsApi as GraphicsApi>::Requirements;

    fn max_texture_side(&self) -> u32;
}
