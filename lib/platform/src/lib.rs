use std::{fmt::Debug, pin::Pin};

use fluxemu_graphics::api::{GraphicsApi, software::Software};

/// A trait abstracting over the various things the platform requires
pub trait Platform: Clone + 'static {
    /// Graphics api in use
    type GraphicsApi: GraphicsApi;
}

/// Test platform
#[derive(Clone, Debug)]
pub struct TestPlatform;

impl Platform for TestPlatform {
    type GraphicsApi = Software;
}

#[cfg(not(target_family = "wasm"))]
pub trait MaybeSend: Send {}
#[cfg(not(target_family = "wasm"))]
impl<T: Send + ?Sized> MaybeSend for T {}

#[cfg(target_family = "wasm")]
pub trait MaybeSend {}
#[cfg(target_family = "wasm")]
impl<T: ?Sized> MaybeSend for T {}

#[cfg(not(target_family = "wasm"))]
pub type MaybeSendBoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;
#[cfg(target_family = "wasm")]
pub type MaybeSendBoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + 'a>>;
