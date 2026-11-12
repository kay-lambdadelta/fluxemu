use fluxemu_environment::Environment;

use crate::{audio::AudioRuntime, graphics::GraphicsRuntime};

/// Extension trait for the platform relevant to the frontend
pub trait Platform: fluxemu_platform::Platform + Sized + 'static {
    /// Audio runtime
    type AudioRuntime: AudioRuntime;

    /// Graphics runtime
    type GraphicsRuntime: GraphicsRuntime<GraphicsApi = Self::GraphicsApi>;

    const EXTERNAL_FILE_DIALOGS_SUPPORTED: bool = false;

    fn save_environment(
        environment: &Environment,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>>;
}
