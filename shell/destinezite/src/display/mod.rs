use fluxemu_frontend::graphics::GraphicsRuntime;
use fluxemu_runtime::graphics::GraphicsRequirements;
use nalgebra::Vector2;

pub mod software;
#[cfg(feature = "webgpu")]
pub mod webgpu;

/// Arbitrary dpi converted to millimeters that produces nice values
#[allow(unused)]
const REFERENCE_PIXEL_PER_MM: f32 = 160.0 / 25.4;

/// Takes the physical size in pixels and the physical dimensions in millimeters and calculates scale factor for egui
#[allow(unused)]
pub fn calculate_scale_factor(
    pixel_dimensions: Vector2<f32>,
    physical_dimensions: Vector2<f32>,
) -> f32 {
    let d_px = pixel_dimensions
        .map(|component| component.powi(2))
        .sum()
        .sqrt();

    let d_mm = physical_dimensions
        .map(|component| component.powi(2))
        .sum()
        .sqrt();

    (d_px / d_mm) / REFERENCE_PIXEL_PER_MM
}

pub trait DisplayContext: Sized + 'static {
    fn dimensions(&self) -> Vector2<u32>;
    fn pre_present_notify(&self) {}
}

pub trait RuntimeAssociatedDisplayContext<R: GraphicsRuntime>: DisplayContext {
    type ProduceDataArgs<'a>;

    fn produce_runtime<'a>(
        &self,
        graphics_requirements: GraphicsRequirements<<R as GraphicsRuntime>::GraphicsApi>,
        args: Self::ProduceDataArgs<'a>,
    ) -> R;
}
