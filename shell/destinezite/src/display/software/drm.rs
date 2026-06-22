use std::sync::Arc;

use drm::{
    buffer::{Buffer, DrmFourcc},
    control::{
        Device, PageFlipFlags,
        dumbbuffer::{DumbBuffer, DumbMapping},
    },
};
use fluxemu_egui_software_renderer::Renderer;
use fluxemu_graphics::api::software::{
    Software,
    texture::{AsTexture, AsTextureMut, RefMutTexture, RefTexture},
};
use fluxemu_runtime::graphics::GraphicsRequirements;
use libseat::Seat;
use nalgebra::Vector2;
use nix::{
    poll::PollTimeout,
    sys::epoll::{Epoll, EpollCreateFlags, EpollEvent, EpollFlags},
};
use palette::{cast::Packed, rgb::channels::Bgra};

use crate::{
    display::{
        RuntimeAssociatedDisplayContext,
        software::{SoftwareCompatibleDisplayContext, SoftwareGraphicsRuntime},
    },
    event_loop::drm::DrmContext,
};

pub struct Surface {
    buffers: [DumbBuffer; 2],
    framebuffers: [drm::control::framebuffer::Handle; 2],
    epoll: Epoll,
    on_back_buffer: bool,
}

pub struct SurfaceBufferGuard<'a> {
    dimensions: Vector2<u16>,
    stride: u32,
    buffer: DumbMapping<'a>,
}

impl AsTexture<Packed<Bgra, [u8; 4]>> for SurfaceBufferGuard<'_> {
    fn as_texture(&self) -> RefTexture<'_, Packed<Bgra, [u8; 4]>> {
        RefTexture::from_storage_with_stride(
            self.dimensions.x as usize,
            self.dimensions.y as usize,
            self.stride as usize,
            bytemuck::cast_slice(self.buffer.as_ref()),
        )
    }
}

impl AsTextureMut<Packed<Bgra, [u8; 4]>> for SurfaceBufferGuard<'_> {
    fn as_texture_mut(&mut self) -> RefMutTexture<'_, Packed<Bgra, [u8; 4]>> {
        RefMutTexture::from_storage_with_stride(
            self.dimensions.x as usize,
            self.dimensions.y as usize,
            self.stride as usize,
            bytemuck::cast_slice_mut(self.buffer.as_mut()),
        )
    }
}

impl RuntimeAssociatedDisplayContext<SoftwareGraphicsRuntime<Self>> for Arc<DrmContext> {
    type ProduceDataArgs<'a> = &'a mut Seat;

    fn produce_runtime(
        &self,
        _graphics_requirements: GraphicsRequirements<Software>,
        _seat: &mut Seat,
    ) -> SoftwareGraphicsRuntime<Self> {
        let (width, height) = self.params.mode.size();

        let buffer_0 = self
            .card
            .create_dumb_buffer((width as u32, height as u32), DrmFourcc::Bgrx8888, 32)
            .unwrap();
        let framebuffer_handle_0 = self.card.add_framebuffer(&buffer_0, 32, 32).unwrap();

        let buffer_1 = self
            .card
            .create_dumb_buffer((width as u32, height as u32), DrmFourcc::Bgrx8888, 32)
            .unwrap();
        let framebuffer_handle_1 = self.card.add_framebuffer(&buffer_1, 32, 32).unwrap();

        self.card
            .set_crtc(
                self.params.crtc_handle,
                Some(framebuffer_handle_0),
                (0, 0),
                &[self.params.connector_handle],
                Some(self.params.mode),
            )
            .unwrap();

        let epoll = Epoll::new(EpollCreateFlags::EPOLL_CLOEXEC).unwrap();
        epoll
            .add(&self.card, EpollEvent::new(EpollFlags::EPOLLIN, 0))
            .unwrap();

        SoftwareGraphicsRuntime {
            renderer: Renderer::default(),
            surface: Surface {
                buffers: [buffer_0, buffer_1],
                framebuffers: [framebuffer_handle_0, framebuffer_handle_1],
                epoll,
                on_back_buffer: true,
            },
            display_handle: self.clone(),
        }
    }
}

impl SoftwareCompatibleDisplayContext for Arc<DrmContext> {
    type MappingError = std::io::Error;
    type PresentError = std::io::Error;
    type ResizeError = std::io::Error;
    type Surface = Surface;

    fn resize_surface(&self, _surface: &mut Self::Surface) -> Result<(), Self::ResizeError> {
        unimplemented!()
    }

    #[inline]
    fn map_surface_buffer<'a>(
        &'a self,
        surface: &'a mut Self::Surface,
    ) -> Result<impl AsTextureMut<Packed<Bgra, [u8; 4]>> + 'a, Self::MappingError> {
        let back_buffer = &mut surface.buffers[surface.on_back_buffer as usize];

        let (width, height) = self.params.mode.size();
        let stride = back_buffer.pitch() / size_of::<Packed<Bgra, [u8; 4]>>() as u32;

        let mapped_buffer = self.card.map_dumb_buffer(back_buffer)?;

        Ok(SurfaceBufferGuard {
            buffer: mapped_buffer,
            dimensions: Vector2::new(width, height),
            stride,
        })
    }

    fn present(&self, surface: &mut Self::Surface) -> Result<(), Self::PresentError> {
        let back_framebuffer_handle = surface.framebuffers[surface.on_back_buffer as usize];

        self.card.page_flip(
            self.params.crtc_handle,
            back_framebuffer_handle,
            PageFlipFlags::EVENT,
            None,
        )?;

        loop {
            let mut events = [EpollEvent::new(EpollFlags::EPOLLIN, 0)];
            let num_events = surface.epoll.wait(&mut events, PollTimeout::NONE)?;

            // Wait around for a pageflip and then swap the back index
            if num_events > 0 {
                for event in self.card.receive_events()? {
                    if let drm::control::Event::PageFlip(_) = event {
                        surface.on_back_buffer = !surface.on_back_buffer;

                        return Ok(());
                    }
                }
            }
        }
    }
}
