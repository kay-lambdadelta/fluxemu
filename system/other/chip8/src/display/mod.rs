use std::fmt::Debug;

use fluxemu_graphics::{
    api::GraphicsApi,
    texture::{AsViewTexture, CopyMode, OwnedTexture, Texture},
};
use fluxemu_runtime::{
    ResourcePath, RuntimeHandle,
    component::{
        Component,
        config::{ComponentConfig, LateContext},
    },
    graphics::SimpleDisplayBackend,
    machine::builder::ComponentBuilder,
    platform::Platform,
    scheduler::{
        Frequency, QuantaAllocator,
        task::{FrequencyBased, Mode},
    },
};
use nalgebra::{Point2, Vector2};
use palette::{
    Srgba,
    named::{BLACK, WHITE},
};

mod software;
#[cfg(feature = "webgpu")]
mod webgpu;

const LORES: Vector2<u8> = Vector2::new(64, 32);
const HIRES: Vector2<u8> = Vector2::new(128, 64);

#[derive(Debug)]
pub struct Chip8Display<G: SupportedGraphicsApi> {
    backend: Option<G::Backend>,
    /// The cpu reads this to see if it can continue execution post draw call
    vsync_occurred: bool,
    staging_buffer: OwnedTexture<Srgba<u8>>,
    hires: bool,
    config: Chip8DisplayConfig,
    framebuffer_path: ResourcePath,
}

impl<G: SupportedGraphicsApi> Chip8Display<G> {
    pub fn vsync_occurred(&self) -> bool {
        self.vsync_occurred
    }

    pub fn set_hires(&mut self, is_hires: bool) {
        if self.config.clear_on_resolution_change {
            self.clear_display();
        }

        let mut new_staging_buffer =
            Texture::from_value(HIRES.x as usize, HIRES.y as usize, BLACK.into());

        new_staging_buffer
            .view_mut(
                0..self.staging_buffer.width(),
                0..self.staging_buffer.height(),
            )
            .copy_from(&self.staging_buffer, CopyMode::Nearest);

        self.staging_buffer = new_staging_buffer;
        self.hires = is_hires;
    }

    pub fn draw_supersized_sprite(&mut self, position: Point2<u8>, sprite: [u8; 32]) -> bool {
        tracing::trace!(
            "Drawing sprite at position {} of dimensions 16x16",
            position,
        );

        let screen_size = if self.hires { HIRES } else { LORES };
        let position = Point2::new(position.x % screen_size.x, position.y % screen_size.y).cast();
        self.vsync_occurred = false;

        let mut hit_detection = false;

        for (y, sprite_row) in sprite.chunks(2).enumerate() {
            let row_bits = u16::from_be_bytes([sprite_row[0], sprite_row[1]]);

            for x in 0..16 {
                let sprite_pixel = row_bits & (1 << (15 - x)) != 0;

                let position = position + Vector2::new(x, y);

                if position.x >= screen_size.x as usize || position.y >= screen_size.y as usize {
                    continue;
                }

                let old_sprite_pixel = self.staging_buffer[position] != BLACK.into();
                if sprite_pixel && old_sprite_pixel {
                    hit_detection = true;
                }

                self.staging_buffer[position] = if sprite_pixel ^ old_sprite_pixel {
                    WHITE
                } else {
                    BLACK
                }
                .into();
            }
        }

        hit_detection
    }

    pub fn draw_sprite(&mut self, position: Point2<u8>, sprite: &[u8]) -> bool {
        tracing::trace!(
            "Drawing sprite at position {} of dimensions 8x{}",
            position,
            sprite.len()
        );

        let screen_size = if self.hires { HIRES } else { LORES };
        self.vsync_occurred = false;

        let position = Point2::new(position.x % screen_size.x, position.y % screen_size.y).cast();
        let dimensions = Vector2::new(8, sprite.len());

        if dimensions.min() == 0 {
            return false;
        }
        let mut hit_detection = false;

        for (y, sprite_byte) in sprite.iter().enumerate() {
            for x in 0..8 {
                let sprite_pixel = sprite_byte & (1 << (7 - x)) != 0;

                let position = position + Vector2::new(x, y);
                if position.x >= screen_size.x as usize || position.y >= screen_size.y as usize {
                    continue;
                }

                let old_sprite_pixel = self.staging_buffer[position] != BLACK.into();
                if sprite_pixel && old_sprite_pixel {
                    hit_detection = true;
                }

                self.staging_buffer[position] = if sprite_pixel ^ old_sprite_pixel {
                    WHITE
                } else {
                    BLACK
                }
                .into();
            }
        }

        hit_detection
    }

    pub fn clear_display(&mut self) {
        tracing::trace!("Clearing display");

        self.staging_buffer.fill(BLACK.into());
    }

    #[inline]
    fn task(&mut self, runtime_handle: &RuntimeHandle, quanta_allocator: QuantaAllocator<'_, '_>) {
        let mut commit_staging_buffer = false;

        for _ in quanta_allocator {
            self.vsync_occurred = true;

            commit_staging_buffer = true;
        }

        if commit_staging_buffer {
            runtime_handle.write_framebuffer::<G, _>(&self.framebuffer_path, |framebuffer| {
                self.backend
                    .as_mut()
                    .unwrap()
                    .commit_staging_buffer(self.staging_buffer.as_view(), framebuffer);
            });
        }
    }
}

impl<G: SupportedGraphicsApi> Component for Chip8Display<G> {
    type Event = ();
}

#[derive(Debug, Default)]
pub struct Chip8DisplayConfig {
    pub clear_on_resolution_change: bool,
}

impl<P: Platform<GraphicsApi: SupportedGraphicsApi>> ComponentConfig<P> for Chip8DisplayConfig {
    type Component = Chip8Display<P::GraphicsApi>;

    fn late_initialize(component: &mut Self::Component, data: &LateContext<P>) {
        let backend = <P::GraphicsApi as SupportedGraphicsApi>::Backend::new(
            data.graphics_initialization_data.clone(),
        );
        component.backend = Some(backend);
    }

    fn build_component(
        self,
        component_builder: ComponentBuilder<P, Self::Component>,
    ) -> Result<Self::Component, Box<dyn std::error::Error>> {
        let (_, framebuffer_path) = component_builder
            .task(
                "synchronization",
                Mode::OnDemand,
                FrequencyBased::new(Frequency::from_num(60), Self::Component::task),
            )
            .framebuffer("framebuffer", |component, path| {
                component
                    .backend
                    .as_mut()
                    .unwrap()
                    .produce_initial_framebuffer(path)
            });

        Ok(Chip8Display {
            backend: None,
            hires: false,
            vsync_occurred: false,
            staging_buffer: Texture::from_value(LORES.x as usize, LORES.y as usize, BLACK.into()),
            framebuffer_path,
            config: self,
        })
    }
}

pub(crate) trait SupportedGraphicsApi: GraphicsApi {
    type Backend: SimpleDisplayBackend<GraphicsApi = Self> + Send + Sync + Debug;
}
