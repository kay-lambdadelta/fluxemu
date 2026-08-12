use fluxemu_graphics::api::software::Software;

use crate::runtime::{AudioRuntime, GraphicsRuntime};

#[derive(Debug, Clone)]
pub struct Platform;

impl fluxemu_runtime::platform::Platform for Platform {
    type GraphicsApi = Software;
}

impl fluxemu_frontend::FrontendPlatform for Platform {
    type AudioRuntime = AudioRuntime;
    type GraphicsRuntime = GraphicsRuntime;

    const EXTERNAL_FILE_DIALOGS_SUPPORTED: bool = false;
}
