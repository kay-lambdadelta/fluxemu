use std::{fmt::Debug, marker::PhantomData};

use fluxemu_frontend::{FrontendPlatform, GraphicsRuntime};
use fluxemu_runtime::platform::Platform;

use crate::audio::CpalAudioRuntime;

pub struct DesktopPlatform<R: GraphicsRuntime> {
    _phantom: PhantomData<fn() -> R>,
}

impl<R: GraphicsRuntime> Clone for DesktopPlatform<R> {
    fn clone(&self) -> Self {
        Self {
            _phantom: PhantomData,
        }
    }
}

impl<R: GraphicsRuntime> Debug for DesktopPlatform<R> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DesktopPlatform").finish()
    }
}

impl<R: GraphicsRuntime> Platform for DesktopPlatform<R> {
    type GraphicsApi = R::GraphicsApi;
}

impl<R: GraphicsRuntime> FrontendPlatform for DesktopPlatform<R> {
    type AudioRuntime = CpalAudioRuntime;

    type GraphicsRuntime = R;
}
