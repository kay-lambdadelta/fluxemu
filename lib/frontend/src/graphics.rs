use std::sync::Arc;

use egui::{Context, FullOutput};
use fluxemu_graphics::api::GraphicsApi;
use fluxemu_runtime::machine::Machine;

/// Extension trait for graphics apis
pub trait GraphicsRuntime: Sized + 'static {
    type GraphicsApi: GraphicsApi;

    /// Refresh the surface
    fn refresh_surface(&mut self);

    /// Present this frame as egui ui
    fn present_egui_overlay(&mut self, context: &Context, full_output: FullOutput);

    /// Present the machine on this frame
    fn present_machine(&mut self, machine: &Arc<Machine>);

    /// Graphics data components require
    fn component_initialization_data(
        &self,
    ) -> <Self::GraphicsApi as GraphicsApi>::InitializationData;

    /// Get the requirements this runtime was created with
    fn created_requirements(&self) -> <Self::GraphicsApi as GraphicsApi>::Requirements;

    /// Max texture size supported by this graphics backend
    fn max_texture_side(&self) -> u32;
}
