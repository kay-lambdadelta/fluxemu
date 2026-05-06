use std::fmt::Debug;

use fluxemu_graphics::api::{GraphicsApi, software::Software};

/// A trait abstracting over the various things the platform requires
pub trait Platform: Clone + Debug + 'static {
    /// Graphics api in use
    type GraphicsApi: GraphicsApi;
}

/// Test platform
#[derive(Clone, Debug)]
pub struct TestPlatform;

impl Platform for TestPlatform {
    type GraphicsApi = Software;
}
