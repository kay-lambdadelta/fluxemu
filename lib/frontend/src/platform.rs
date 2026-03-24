use fluxemu_runtime::platform::Platform;

use crate::{AudioRuntime, GraphicsRuntime};

/// Extension trait for the platform relevant to the frontend
pub trait FrontendPlatform: Platform + Sized + 'static {
    /// Audio runtime
    type AudioRuntime: AudioRuntime;

    /// Graphics runtime
    type GraphicsRuntime: GraphicsRuntime<GraphicsApi = Self::GraphicsApi>;
}
