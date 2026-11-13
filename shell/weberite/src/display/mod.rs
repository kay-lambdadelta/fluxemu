use fluxemu_frontend::graphics::GraphicsRuntime;
use fluxemu_graphics::{api::GraphicsApi, shader::ShaderType};
use fluxemu_runtime::graphics::{DisplayGeometry, GraphicsRequirements};
use nalgebra::Vector2;
use softbuffer::SoftBufferError;
use wasm_bindgen::JsCast;
use web_sys::HtmlCanvasElement;

mod software;
#[cfg(feature = "webgpu")]
mod webgpu;

pub use software::SoftwareGraphicsRuntime;
#[cfg(feature = "webgpu")]
pub use webgpu::WebgpuGraphicsRuntime;

const CANVAS_ID: &str = "canvas";
const DISPLAY_ASPECT_RATIO_PROPERTY: &str = "--display-aspect-ratio";

#[derive(Debug, thiserror::Error)]
pub enum DisplayError {
    #[error("This page has no canvas")]
    NoCanvas,
    #[error("Could not modify the canvas: {0:?}")]
    Canvas(wasm_bindgen::JsValue),
    #[error(transparent)]
    Softbuffer(#[from] SoftBufferError),
    #[cfg(feature = "webgpu")]
    #[error(transparent)]
    CreateSurface(#[from] wgpu::CreateSurfaceError),
    #[cfg(feature = "webgpu")]
    #[error(transparent)]
    RequestAdapter(#[from] wgpu::RequestAdapterError),
    #[cfg(feature = "webgpu")]
    #[error(transparent)]
    RequestDevice(#[from] wgpu::RequestDeviceError),
    #[cfg(feature = "webgpu")]
    #[error("The adapter cannot present to this canvas")]
    UnsupportedSurface,
}

pub trait CanvasGraphicsRuntime: GraphicsRuntime {
    fn new(
        canvas: HtmlCanvasElement,
        graphics_requirements: GraphicsRequirements<Self::GraphicsApi>,
    ) -> Result<Self, DisplayError>;

    fn present(
        &mut self,
        framebuffer: &<Self::GraphicsApi as GraphicsApi>::Framebuffer,
        display_geometry: &DisplayGeometry,
        shader: ShaderType,
    ) -> Result<(), DisplayError>;
}

pub struct CanvasAspectRatio {
    canvas: HtmlCanvasElement,
    current: Option<f32>,
}

impl CanvasAspectRatio {
    pub fn new(canvas: HtmlCanvasElement) -> Self {
        Self {
            canvas,
            current: None,
        }
    }

    pub fn update(&mut self, display_size: Vector2<f32>) -> Result<(), DisplayError> {
        let aspect_ratio = display_size.x / display_size.y;

        if !aspect_ratio.is_finite() || self.current == Some(aspect_ratio) {
            return Ok(());
        }

        self.canvas
            .style()
            .set_property(DISPLAY_ASPECT_RATIO_PROPERTY, &aspect_ratio.to_string())
            .map_err(DisplayError::Canvas)?;
        self.current = Some(aspect_ratio);

        Ok(())
    }
}

pub fn find_canvas() -> Result<HtmlCanvasElement, DisplayError> {
    web_sys::window()
        .and_then(|window| window.document())
        .and_then(|document| document.get_element_by_id(CANVAS_ID))
        .and_then(|element| element.dyn_into::<HtmlCanvasElement>().ok())
        .ok_or(DisplayError::NoCanvas)
}

#[cfg(feature = "webgpu")]
pub fn replace_canvas(canvas: &HtmlCanvasElement) -> Result<HtmlCanvasElement, DisplayError> {
    let replacement = canvas
        .clone_node()
        .map_err(DisplayError::Canvas)?
        .dyn_into::<HtmlCanvasElement>()
        .map_err(|node| DisplayError::Canvas(node.into()))?;

    canvas
        .replace_with_with_node_1(&replacement)
        .map_err(DisplayError::Canvas)?;

    Ok(replacement)
}
