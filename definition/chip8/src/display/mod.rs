use std::{
    collections::HashMap,
    fmt::Debug,
    io::{Read, Write},
};

use fluxemu_runtime::{
    RuntimeApi,
    component::{
        Component, ComponentVersion,
        config::{ComponentConfig, LateContext, LateInitializedData},
    },
    graphics::{
        GraphicsApi,
        software::{CopyMode, Texture, TextureImpl, TextureImplMut},
    },
    machine::builder::{ComponentBuilder, SchedulerParticipation},
    path::ResourcePath,
    platform::Platform,
    scheduler::{Period, SynchronizationContext},
};
use nalgebra::{Point2, Vector2};
use palette::{
    Srgba,
    named::{BLACK, WHITE},
};
use serde::{Deserialize, Serialize};

mod software;
#[cfg(feature = "webgpu")]
mod webgpu;

const LORES: Vector2<u8> = Vector2::new(64, 32);
const HIRES: Vector2<u8> = Vector2::new(128, 64);

#[derive(Debug, Serialize, Deserialize)]
struct Snapshot {
    screen_buffer: Texture<Srgba<u8>>,
    vsync_occurred: bool,
    hires: bool,
}

#[derive(Debug)]
pub struct Chip8Display<G: SupportedGraphicsApiChip8Display> {
    backend: Option<G::Backend>,
    /// The cpu reads this to see if it can continue execution post draw call
    vsync_occurred: bool,
    staging_buffer: Texture<Srgba<u8>>,
    hires: bool,
    framebuffer_path: ResourcePath,
    config: Chip8DisplayConfig,
}

impl<G: SupportedGraphicsApiChip8Display> Chip8Display<G> {
    pub fn vsync_occurred(&self) -> bool {
        self.vsync_occurred
    }

    pub fn set_hires(&mut self, is_hires: bool) {
        if self.config.clear_on_resolution_change {
            self.clear_display();
        }

        let mut new_staging_buffer = Texture::new(HIRES.x as usize, HIRES.y as usize, BLACK.into());

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
}

impl<G: SupportedGraphicsApiChip8Display> Component for Chip8Display<G> {
    fn load_snapshot(
        &mut self,
        version: ComponentVersion,
        reader: &mut dyn Read,
    ) -> Result<(), Box<dyn std::error::Error>> {
        assert_eq!(version, 0);
        let snapshot: Snapshot = rmp_serde::decode::from_read(reader)?;
        self.set_hires(snapshot.hires);

        self.staging_buffer
            .copy_from(&snapshot.screen_buffer, CopyMode::Nearest);

        self.vsync_occurred = snapshot.vsync_occurred;

        Ok(())
    }

    fn store_snapshot(&self, mut writer: &mut dyn Write) -> Result<(), Box<dyn std::error::Error>> {
        let snapshot = Snapshot {
            screen_buffer: self.staging_buffer.clone(),
            hires: self.hires,
            vsync_occurred: self.vsync_occurred,
        };

        rmp_serde::encode::write_named(&mut writer, &snapshot)?;

        Ok(())
    }

    fn synchronize(&mut self, mut context: SynchronizationContext) {
        let mut commit_staging_buffer = false;

        for _ in context.allocate(Period::ONE / 60, None) {
            self.vsync_occurred = true;

            commit_staging_buffer = true;
        }

        if commit_staging_buffer {
            let runtime = RuntimeApi::current();

            // Commit the framebuffer
            runtime.commit_framebuffer::<G>(&self.framebuffer_path, |framebuffer| {
                self.backend
                    .as_mut()
                    .unwrap()
                    .commit_staging_buffer(&self.staging_buffer, framebuffer);
            });
        }
    }

    fn needs_work(&self, delta: Period) -> bool {
        delta >= Period::ONE / 60
    }
}

pub(crate) trait Chip8DisplayBackend: Send + Sync + Debug + 'static {
    type GraphicsApi: GraphicsApi;

    fn new(initialization_data: <Self::GraphicsApi as GraphicsApi>::InitializationData) -> Self;
    fn create_framebuffer(&self) -> <Self::GraphicsApi as GraphicsApi>::Framebuffer;
    fn commit_staging_buffer(
        &mut self,
        staging_buffer: &Texture<Srgba<u8>>,
        framebuffer: &mut <Self::GraphicsApi as GraphicsApi>::Framebuffer,
    );
}

#[derive(Debug, Default)]
pub struct Chip8DisplayConfig {
    pub clear_on_resolution_change: bool,
}

impl<P: Platform<GraphicsApi: SupportedGraphicsApiChip8Display>> ComponentConfig<P>
    for Chip8DisplayConfig
{
    type Component = Chip8Display<P::GraphicsApi>;

    fn late_initialize(
        component: &mut Self::Component,
        data: &LateContext<P>,
    ) -> LateInitializedData<P> {
        let backend = <P::GraphicsApi as SupportedGraphicsApiChip8Display>::Backend::new(
            data.graphics_initialization_data.clone(),
        );
        let framebuffer = backend.create_framebuffer();
        component.backend = Some(backend);

        let framebuffer_name = component.framebuffer_path.name().to_string().into();

        LateInitializedData {
            framebuffers: HashMap::from_iter([(framebuffer_name, framebuffer)]),
        }
    }

    fn build_component(
        self,
        component_builder: ComponentBuilder<'_, '_, P, Self::Component>,
    ) -> Result<Self::Component, Box<dyn std::error::Error>> {
        let (_, framebuffer_path) = component_builder
            .scheduler_participation(Some(SchedulerParticipation::OnAccess))
            .framebuffer("framebuffer");

        Ok(Chip8Display {
            backend: None,
            hires: false,
            vsync_occurred: false,
            staging_buffer: Texture::new(LORES.x as usize, LORES.y as usize, BLACK.into()),
            framebuffer_path,
            config: self,
        })
    }
}

pub(crate) trait SupportedGraphicsApiChip8Display: GraphicsApi {
    type Backend: Chip8DisplayBackend<GraphicsApi = Self>;
}
