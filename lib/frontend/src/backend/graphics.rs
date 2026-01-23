use std::fmt::Debug;

use egui::FullOutput;
use fluxemu_runtime::{graphics::GraphicsApi, machine::Machine, platform::Platform};
use nalgebra::Vector2;

use crate::environment::Environment;

/// Handle to the surface we will be rendering graphics to
pub trait WindowingHandle: Clone + Debug + 'static {
    /// Get the physical size of the window in pixels
    fn physical_size(&self) -> Vector2<u32>;
    /// Get the logical scale factor of the window
    fn scale(&self) -> f64;

    /// Compute the logical size of the window
    fn logical_size(&self) -> Vector2<f64> {
        self.physical_size().cast::<f64>() / self.scale()
    }
}

/// Extension trait for graphics apis
pub trait GraphicsRuntime<P: Platform>: Debug + Sized + 'static {
    /// The type of display api handle that is required here
    type WindowingHandle: WindowingHandle;

    /// Create the graphics runtime
    fn new(
        display_api_handle: Self::WindowingHandle,
        required_features: <P::GraphicsApi as GraphicsApi>::Features,
        preferred_features: <P::GraphicsApi as GraphicsApi>::Features,
        environment: &Environment,
    ) -> Result<Self, Box<dyn std::error::Error>>;

    /// Graphics data components require
    fn component_initialization_data(&self) -> <P::GraphicsApi as GraphicsApi>::InitializationData;

    /// Draw the next frame
    fn redraw(
        &mut self,
        egui_context: &egui::Context,
        full_output: FullOutput,
        machine: Option<&Machine>,
        environment: &Environment,
    );

    /// Get the max side that a texture can have on this rendering device
    ///
    /// Use u32::MAX to indicate no limit (probably only exists for software renderers)
    fn max_texture_side_size(&self) -> u32 {
        // Overly safe default
        2048
    }

    /// Notification that the render surface resized
    fn display_resized(&mut self) {}
}
