use std::{fmt::Debug, marker::PhantomData};

use fluxemu_environment::Environment;
use fluxemu_frontend::graphics::GraphicsRuntime;

use crate::runtime::AudioRuntime;

pub struct Platform<R: GraphicsRuntime> {
    _phantom: PhantomData<fn() -> R>,
}

impl<R: GraphicsRuntime> Clone for Platform<R> {
    fn clone(&self) -> Self {
        Self {
            _phantom: PhantomData,
        }
    }
}

impl<R: GraphicsRuntime> Debug for Platform<R> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Platform").finish()
    }
}

impl<R: GraphicsRuntime> fluxemu_platform::Platform for Platform<R> {
    type GraphicsApi = R::GraphicsApi;
}

impl<R: GraphicsRuntime> fluxemu_frontend::Platform for Platform<R> {
    type AudioRuntime = AudioRuntime;
    type GraphicsRuntime = R;

    // Not really exercised here but technically it does
    const EXTERNAL_FILE_DIALOGS_SUPPORTED: bool = true;

    fn save_environment(
        environment: &Environment,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        fluxemu_paths::save_environment(environment)
    }
}
