//! Main graphics definition things for fluxemu

use std::{fmt::Debug, ops::BitOr};

use fluxemu_graphics::{
    api::{Framebuffer, GraphicsApi},
    texture::RefTexture,
};
use fluxemu_math::rectangle::Rectangle;
use nalgebra::{Point2, Vector2};
pub use num::rational::Ratio;
use palette::Srgba;
use serde::{Deserialize, Serialize};

use crate::ResourcePath;

mod backend;
mod job;
mod staging;

pub use backend::{DisplayBackend, DisplayBackendSelector, SoftwareFallback};
pub use job::{Gpu, Handle, JobQueue, Resources, Submitter};
pub use staging::StagingPool;

/// Version specifier for graphics apis
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct GraphicsVersion {
    /// Major
    pub major: u32,
    /// Minor
    pub minor: u32,
}

/// The requirements for a graphics context
#[derive(Debug)]
pub struct GraphicsRequirements<G: GraphicsApi> {
    /// Requirements that are needed for basic operation, failure is allowed if these are not present
    ///
    /// It is recommended there be as little of these as possible
    pub required: G::Requirements,
    /// Requirements that are nice to have, operation may continue without them
    pub preferred: G::Requirements,
}

impl<G: GraphicsApi> Clone for GraphicsRequirements<G> {
    fn clone(&self) -> Self {
        Self {
            required: self.required.clone(),
            preferred: self.preferred.clone(),
        }
    }
}

impl<G: GraphicsApi> BitOr for GraphicsRequirements<G> {
    type Output = GraphicsRequirements<G>;

    fn bitor(self, rhs: Self) -> Self::Output {
        Self {
            required: self.required | rhs.required,
            preferred: self.preferred | rhs.preferred,
        }
    }
}

impl<G: GraphicsApi> Default for GraphicsRequirements<G> {
    fn default() -> Self {
        Self {
            required: Default::default(),
            preferred: Default::default(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DisplayGeometry {
    pub pixel_aspect_ratio: Ratio<u32>,
    pub display_rectangle: Option<Rectangle<usize>>,
}

impl DisplayGeometry {
    pub const SQUARE: Self = Self {
        pixel_aspect_ratio: Ratio::ONE,
        display_rectangle: None,
    };

    pub fn resolve_display_rectangle(&self, framebuffer: &impl Framebuffer) -> Rectangle<usize> {
        let size = Point2::new(framebuffer.width(), framebuffer.height());

        match self.display_rectangle {
            Some(display_rectangle) => Rectangle::from_min_and_max(
                display_rectangle.min.inf(&size),
                display_rectangle.max.inf(&size),
            ),
            None => Rectangle::from_size(size.coords),
        }
    }

    pub fn display_size(&self, framebuffer: &impl Framebuffer) -> Vector2<f32> {
        let size = self
            .resolve_display_rectangle(framebuffer)
            .size()
            .cast::<f32>();

        Vector2::new(
            size.x * *self.pixel_aspect_ratio.numer() as f32
                / *self.pixel_aspect_ratio.denom() as f32,
            size.y,
        )
    }
}

impl Default for DisplayGeometry {
    fn default() -> Self {
        Self::SQUARE
    }
}

/// A display backend for components which mostly commit a CPU side framebuffer
pub trait SimpleDisplayBackend: Debug {
    type GraphicsApi: GraphicsApi;

    fn produce_initial_framebuffer(
        &mut self,
        path: &ResourcePath,
    ) -> Handle<<Self::GraphicsApi as GraphicsApi>::Framebuffer>;
    fn commit_staging_buffer(
        &mut self,
        staging_buffer: RefTexture<Srgba<u8>>,
        framebuffer: Handle<<Self::GraphicsApi as GraphicsApi>::Framebuffer>,
    );
}
