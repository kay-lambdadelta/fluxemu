use std::marker::PhantomData;

use fluxemu_environment::Environment;
use fluxemu_frontend::graphics::GraphicsRuntime;
use fluxemu_platform::Platform;

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

impl<R: GraphicsRuntime, const EXTERNAL_FILE_DIALOGS_SUPPORTED: bool> fluxemu_frontend::Platform
    for DesktopPlatform<R, EXTERNAL_FILE_DIALOGS_SUPPORTED>
{
    type AudioRuntime = CpalAudioRuntime;
    type GraphicsRuntime = R;

    const EXTERNAL_FILE_DIALOGS_SUPPORTED: bool = EXTERNAL_FILE_DIALOGS_SUPPORTED;

    fn save_environment(
        environment: &Environment,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        fluxemu_paths::save_environment(environment)
    }
}
