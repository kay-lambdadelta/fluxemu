use std::num::NonZero;

use fluxemu_graphics::texture::{AsViewTexture, CopyMode, RefMutTexture};
use nalgebra::Vector2;
use palette::{Srgba, cast::Packed, rgb::channels::Bgra};
use softbuffer::{NoDisplayHandle, NoWindowHandle, SoftBufferError, Surface, SurfaceExtWeb};
use wasm_bindgen::JsCast;
use web_sys::HtmlCanvasElement;

const CANVAS_ID: &str = "canvas";

#[derive(Debug, thiserror::Error)]
pub enum DisplayError {
    #[error("This page has no canvas")]
    NoCanvas,
    #[error(transparent)]
    Surface(#[from] SoftBufferError),
}

pub struct CanvasDisplay {
    surface: Surface<NoDisplayHandle, NoWindowHandle>,
    dimensions: Option<Vector2<usize>>,
}

impl CanvasDisplay {
    pub fn new() -> Result<Self, DisplayError> {
        let canvas = web_sys::window()
            .and_then(|window| window.document())
            .and_then(|document| document.get_element_by_id(CANVAS_ID))
            .and_then(|element| element.dyn_into::<HtmlCanvasElement>().ok())
            .ok_or(DisplayError::NoCanvas)?;

        let surface = Surface::from_canvas(canvas)?;

        Ok(Self {
            surface,
            dimensions: None,
        })
    }

    pub fn present(&mut self, frame: impl AsViewTexture<Srgba<u8>>) -> Result<(), SoftBufferError> {
        let frame = frame.as_view();
        let dimensions = frame.size();

        let (Some(width), Some(height)) = (
            NonZero::new(dimensions.x as u32),
            NonZero::new(dimensions.y as u32),
        ) else {
            return Ok(());
        };

        if self.dimensions != Some(dimensions) {
            self.surface.resize(width, height)?;
            self.dimensions = Some(dimensions);
        }

        let mut buffer = self.surface.buffer_mut()?;
        let mut target = RefMutTexture::<Packed<Bgra, [u8; 4]>>::from_storage(
            dimensions.x,
            dimensions.y,
            bytemuck::cast_slice_mut(&mut buffer),
        );
        target.map_from(frame, CopyMode::Nearest, From::from);

        buffer.present()
    }
}
