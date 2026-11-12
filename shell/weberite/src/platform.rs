use fluxemu_environment::Environment;
use fluxemu_frontend::graphics::PassthroughGraphicsRuntime;
use fluxemu_graphics::api::software::Software;

use crate::runtime::AudioRuntime;

#[derive(Clone, Debug)]
pub struct Platform;

impl fluxemu_platform::Platform for Platform {
    type GraphicsApi = Software;
}

impl fluxemu_frontend::Platform for Platform {
    type AudioRuntime = AudioRuntime;
    type GraphicsRuntime = PassthroughGraphicsRuntime;

    const EXTERNAL_FILE_DIALOGS_SUPPORTED: bool = true;

    fn save_environment(
        environment: &Environment,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        fluxemu_paths::save_environment(environment)
    }
}
