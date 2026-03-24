use std::{fmt::Debug, ops::BitOr};

use crate::graphics::GraphicsApi;

/// The requirements for a graphics context
#[derive(Debug)]
pub struct GraphicsRequirements<G: GraphicsApi> {
    pub required: G::Requirements,
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
