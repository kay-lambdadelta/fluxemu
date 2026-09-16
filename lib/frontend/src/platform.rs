use crate::{audio::AudioRuntime, graphics::GraphicsRuntime};

/// Extension trait for the platform relevant to the frontend
pub trait Platform: fluxemu_runtime::platform::Platform + Sized + 'static {
    /// Audio runtime
    type AudioRuntime: AudioRuntime;

    /// Graphics runtime
    type GraphicsRuntime: GraphicsRuntime<GraphicsApi = Self::GraphicsApi>;

    const EXTERNAL_FILE_DIALOGS_SUPPORTED: bool = false;
}
