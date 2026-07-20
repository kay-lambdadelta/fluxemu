use std::marker::PhantomData;

use fluxemu_frontend::{FrontendPlatform, graphics::GraphicsRuntime};
use fluxemu_runtime::platform::Platform;

use crate::audio::CpalAudioRuntime;

pub struct DesktopPlatform<R: GraphicsRuntime, const EXTERNAL_FILE_DIALOGS_SUPPORTED: bool> {
    _phantom: PhantomData<fn() -> R>,
}

impl<R: GraphicsRuntime, const EXTERNAL_FILE_DIALOGS_SUPPORTED: bool> Clone
    for DesktopPlatform<R, EXTERNAL_FILE_DIALOGS_SUPPORTED>
{
    fn clone(&self) -> Self {
        Self {
            _phantom: PhantomData,
        }
    }
}

impl<R: GraphicsRuntime, const EXTERNAL_FILE_DIALOGS_SUPPORTED: bool> Platform
    for DesktopPlatform<R, EXTERNAL_FILE_DIALOGS_SUPPORTED>
{
    type GraphicsApi = R::GraphicsApi;
}

impl<R: GraphicsRuntime, const EXTERNAL_FILE_DIALOGS_SUPPORTED: bool> FrontendPlatform
    for DesktopPlatform<R, EXTERNAL_FILE_DIALOGS_SUPPORTED>
{
    type AudioRuntime = CpalAudioRuntime;
    type GraphicsRuntime = R;
    const EXTERNAL_FILE_DIALOGS_SUPPORTED: bool = EXTERNAL_FILE_DIALOGS_SUPPORTED;
}
