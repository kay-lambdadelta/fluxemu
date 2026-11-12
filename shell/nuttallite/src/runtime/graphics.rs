use std::io;

use fluxemu_frontend::graphics::DisplayContext;
use fluxemu_frontend_egui::rendering::software::{
    SoftwareCompatibleDisplayContext, SoftwareGraphicsRuntime,
};
use fluxemu_graphics::texture::AsViewTextureMut;
use nalgebra::Vector2;

use crate::runtime::framebuffer::{Framebuffer, FramebufferError, Pixel};

pub type GraphicsRuntime = SoftwareGraphicsRuntime<FramebufferDisplay, 2>;

pub fn open_graphics_runtime() -> Result<GraphicsRuntime, FramebufferError> {
    let framebuffer = Framebuffer::open()?;
    let front_page = framebuffer.front_page();
    let display = FramebufferDisplay {
        dimensions: Vector2::new(front_page.width() as u32, front_page.height() as u32),
    };

    Ok(GraphicsRuntime::new(display, framebuffer))
}

#[derive(Debug, Clone, Copy)]
pub struct FramebufferDisplay {
    dimensions: Vector2<u32>,
}

impl DisplayContext for FramebufferDisplay {
    fn dimensions(&self) -> Vector2<u32> {
        self.dimensions
    }
}

impl SoftwareCompatibleDisplayContext for FramebufferDisplay {
    type MappingError = io::Error;
    type Pixel = Pixel;
    type PresentError = io::Error;
    type ResizeError = io::Error;
    type Surface = Framebuffer;

    fn resize_surface(&self, _surface: &mut Self::Surface) -> Result<(), Self::ResizeError> {
        Ok(())
    }

    fn map_surface_buffer<'a>(
        &'a self,
        surface: &'a mut Self::Surface,
    ) -> Result<impl AsViewTextureMut<Self::Pixel> + 'a, Self::MappingError> {
        surface.wait_for_back_page()?;

        Ok(surface.back_page())
    }

    fn present(&self, surface: &mut Self::Surface) -> Result<(), Self::PresentError> {
        surface.present()
    }
}
