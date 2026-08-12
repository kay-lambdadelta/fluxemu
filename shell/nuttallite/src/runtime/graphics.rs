use fluxemu_egui_software_renderer::Renderer;
use fluxemu_frontend::graphics::{DrawTarget, present_machine_software};
use fluxemu_graphics::api::{
    GraphicsApi,
    software::{Software, texture::OwnedTexture},
};
use fluxemu_runtime::graphics::GraphicsRequirements;
use palette::{Srgb, Srgba};

pub struct GraphicsRuntime {
    egui_renderer: Renderer,
    pub texture: OwnedTexture<Srgba<u8>>,
}

impl Default for GraphicsRuntime {
    fn default() -> Self {
        Self {
            egui_renderer: Renderer::default(),
            texture: OwnedTexture::new(256, 256),
        }
    }
}

impl fluxemu_frontend::graphics::GraphicsRuntime for GraphicsRuntime {
    type GraphicsApi = Software;

    fn reconfigure(&mut self, _graphics_requirements: GraphicsRequirements<Self::GraphicsApi>) {}

    fn refresh_surface(&mut self) {
        todo!()
    }

    fn present<'a>(
        &'a mut self,
        clear_color: Srgb<u8>,
        targets: impl IntoIterator<Item = DrawTarget<'a>>,
    ) {
        self.texture.fill(clear_color.into());

        for target in targets {
            match target {
                DrawTarget::Egui {
                    context,
                    full_output,
                } => {
                    self.egui_renderer
                        .render::<_, 8>(context, full_output, &mut self.texture);
                }
                DrawTarget::Machine { machine } => {
                    present_machine_software(machine, &mut self.texture);
                }
            }
        }
    }

    fn component_initialization_data(
        &self,
    ) -> <Self::GraphicsApi as GraphicsApi>::InitializationData {
    }

    fn max_texture_side(&self) -> u32 {
        u32::MAX
    }
}
