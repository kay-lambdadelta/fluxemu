use std::sync::Arc;

use fluxemu_frontend::{GraphicsRuntime, software::EguiRenderer};
use fluxemu_runtime::{
    graphics::{
        GraphicsApi,
        software::{Requirements, Software, TextureImplMut, TextureViewMut},
    },
    machine::{Machine, graphics::GraphicsRequirements},
};
use palette::{cast::Packed, named::BLACK, rgb::channels::Bgra};
use softbuffer::{Context, Surface};
use winit::window::Window;

use crate::windowing::WinitCompatibleGraphicsRuntime;

pub struct SoftwareGraphicsRuntime {
    renderer: EguiRenderer,
    surface: Surface<Arc<Window>, Arc<Window>>,
}

impl GraphicsRuntime for SoftwareGraphicsRuntime {
    type GraphicsApi = Software;

    fn refresh_surface(&mut self) {
        let window_dimensions = self.surface.window().inner_size();

        self.surface
            .resize(
                window_dimensions.width.try_into().unwrap(),
                window_dimensions.height.try_into().unwrap(),
            )
            .unwrap();
    }

    fn present_egui_overlay(&mut self, context: &egui::Context, full_output: egui::FullOutput) {
        if let Ok(mut surface_buffer) = self.surface.buffer_mut() {
            let width = surface_buffer.width();
            let height = surface_buffer.height();

            let mut surface_texture = TextureViewMut::from_slice(
                bytemuck::cast_slice_mut(&mut surface_buffer),
                width.get() as usize,
                height.get() as usize,
            );
            surface_texture.fill(BLACK.into());

            self.renderer
                .render::<Bgra>(context, full_output, surface_texture);

            surface_buffer.present().unwrap();
        }
    }

    fn present_machine(&mut self, machine: &Arc<Machine>) {
        if let Ok(mut surface_buffer) = self.surface.buffer_mut() {
            let width = surface_buffer.width();
            let height = surface_buffer.height();

            let mut surface_texture: TextureViewMut<'_, Packed<Bgra, [u8; 4]>> =
                TextureViewMut::from_slice(
                    bytemuck::cast_slice_mut(&mut surface_buffer),
                    width.get() as usize,
                    height.get() as usize,
                );
            surface_texture.fill(BLACK.into());

            let framebuffers = machine.framebuffers();

            for (display_path, framebuffer) in framebuffers.iter() {
                // Ensure we are at least on this frame for this component
                machine.interact_dyn(display_path.parent().unwrap(), |_| {});

                let framebuffer_guard = framebuffer.lock().unwrap();
                let framebuffer_texture: &<Self::GraphicsApi as GraphicsApi>::Framebuffer =
                    framebuffer_guard.downcast_ref().unwrap();

                let mut framebuffer_texture = framebuffer_texture.clone();
                framebuffer_texture.rescale_nearest(width.get() as usize, height.get() as usize);

                surface_texture.copy_from(&framebuffer_texture, .., ..);
            }

            surface_buffer.present().unwrap();
        }
    }

    fn component_initialization_data(
        &self,
    ) -> <Self::GraphicsApi as fluxemu_runtime::graphics::GraphicsApi>::InitializationData {
    }

    fn created_requirements(
        &self,
    ) -> <Self::GraphicsApi as fluxemu_runtime::graphics::GraphicsApi>::Requirements {
        Requirements
    }

    fn max_texture_side(&self) -> u32 {
        u32::MAX
    }
}

impl WinitCompatibleGraphicsRuntime for SoftwareGraphicsRuntime {
    fn new(window: Arc<Window>, _requirements: GraphicsRequirements<Self::GraphicsApi>) -> Self {
        let context = Context::new(window.clone()).unwrap();
        let mut surface = Surface::new(&context, window.clone()).unwrap();

        // Set initial size

        let window_dimensions = window.inner_size();

        surface
            .resize(
                window_dimensions.width.try_into().unwrap(),
                window_dimensions.height.try_into().unwrap(),
            )
            .unwrap();

        Self {
            surface,
            renderer: EguiRenderer::default(),
        }
    }
}
