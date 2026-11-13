use std::num::NonZero;

use fluxemu_frontend::graphics::GraphicsRuntime;
use fluxemu_graphics::{
    api::software::Software,
    shader::ShaderType,
    texture::{AsViewTexture, CopyMode, CowTexture, OwnedTexture, RefMutTexture},
};
use fluxemu_runtime::graphics::{DisplayGeometry, GraphicsRequirements};
use nalgebra::Vector2;
use palette::{Srgba, cast::Packed, rgb::channels::Bgra};
use softbuffer::{NoDisplayHandle, NoWindowHandle, Surface, SurfaceExtWeb};
use web_sys::HtmlCanvasElement;

use crate::display::{CanvasAspectRatio, CanvasGraphicsRuntime, DisplayError};

pub struct SoftwareGraphicsRuntime {
    surface: Surface<NoDisplayHandle, NoWindowHandle>,
    dimensions: Option<Vector2<usize>>,
    aspect_ratio: CanvasAspectRatio,
}

impl GraphicsRuntime for SoftwareGraphicsRuntime {
    type GraphicsApi = Software;

    fn reconfigure(&mut self, _graphics_requirements: GraphicsRequirements<Self::GraphicsApi>) {}

    fn refresh_surface(&mut self) {}

    fn with_context<T>(&mut self, callback: impl FnOnce(&mut ()) -> T) -> T {
        callback(&mut ())
    }

    fn max_texture_side(&self) -> u32 {
        u32::MAX
    }

    fn screenshot<'a>(
        &self,
        framebuffer: &'a OwnedTexture<Srgba<u8>>,
    ) -> Option<CowTexture<'a, Srgba<u8>>> {
        Some(framebuffer.as_view().into())
    }
}

impl CanvasGraphicsRuntime for SoftwareGraphicsRuntime {
    fn new(
        canvas: HtmlCanvasElement,
        _graphics_requirements: GraphicsRequirements<Software>,
    ) -> Result<Self, DisplayError> {
        let aspect_ratio = CanvasAspectRatio::new(canvas.clone());
        let surface = Surface::from_canvas(canvas)?;

        Ok(Self {
            surface,
            dimensions: None,
            aspect_ratio,
        })
    }

    fn present(
        &mut self,
        framebuffer: &OwnedTexture<Srgba<u8>>,
        display_geometry: &DisplayGeometry,
        _shader: ShaderType,
    ) -> Result<(), DisplayError> {
        let display_rectangle = display_geometry.resolve_display_rectangle(framebuffer);
        let frame = framebuffer.view(display_rectangle.x_range(), display_rectangle.y_range());
        let dimensions = frame.size();

        let (Some(width), Some(height)) = (
            NonZero::new(dimensions.x as u32),
            NonZero::new(dimensions.y as u32),
        ) else {
            return Ok(());
        };

        self.aspect_ratio
            .update(display_geometry.display_size(framebuffer))?;

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

        Ok(buffer.present()?)
    }
}
