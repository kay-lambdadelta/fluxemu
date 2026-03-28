use std::fmt::Debug;

use palette::Srgb;

use crate::graphics::{GraphicsApi, software::Software};

/// A trait abstracting over the various things the platform requires
pub trait Platform: Clone + Debug + 'static {
    /// Graphics api in use
    type GraphicsApi: GraphicsApi;
}

#[derive(Clone, Debug)]
/// Test platform
pub struct TestPlatform;

impl Platform for TestPlatform {
    type GraphicsApi = Software;
}
