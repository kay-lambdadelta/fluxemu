use std::sync::Arc;

use drm::{
    buffer::{Buffer, DrmFourcc},
    control::{Device, PageFlipFlags, dumbbuffer::DumbBuffer},
};
use fluxemu_egui_software_renderer::Renderer;
use fluxemu_graphics::api::software::{Software, texture::TextureViewMut};
use fluxemu_runtime::graphics::GraphicsRequirements;
use libseat::Seat;
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
    type Surface = Surface;

    type ResizeError = std::io::Error;
    type MappingError = std::io::Error;

    fn resize_surface(&self, _surface: &mut Self::Surface) -> Result<(), Self::ResizeError> {
        unimplemented!()
    }

    fn map_surface_buffer(
        &self,
        surface: &mut Self::Surface,
        callback: impl FnOnce(TextureViewMut<'_, Packed<Bgra, [u8; 4]>>),
    ) -> Result<(), Self::MappingError> {
        let (width, height) = self.params.mode.size();

        let (back_buffer, back_framebuffer_handle) = (
            &mut surface.buffers[surface.on_back_buffer as usize],
            surface.framebuffers[surface.on_back_buffer as usize],
        );

        let stride = back_buffer.pitch() / size_of::<Packed<Bgra, [u8; 4]>>() as u32;

        let mut mapped_buffer = self.card.map_dumb_buffer(back_buffer)?;
        let texture_view = TextureViewMut::from_slice_with_stride(
            bytemuck::cast_slice_mut(&mut mapped_buffer),
            width as usize,
            height as usize,
            stride as usize,
        );

        callback(texture_view);
        drop(mapped_buffer);

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
