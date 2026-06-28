//! Main graphics definition things for fluxemu

use std::ops::BitOr;

use fluxemu_graphics::api::GraphicsApi;
use serde::{Deserialize, Serialize};

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
