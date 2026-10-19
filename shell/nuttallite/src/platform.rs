use std::marker::PhantomData;

use fluxemu_frontend::graphics::GraphicsRuntime;
use fluxemu_graphics::api::software::Software;

use crate::runtime::AudioRuntime;

#[derive(Debug)]
pub struct Platform<R: GraphicsRuntime<GraphicsApi = Software>> {
    _phantom: PhantomData<fn() -> R>,
}

impl<R: GraphicsRuntime<GraphicsApi = Software>> Clone for Platform<R> {
    fn clone(&self) -> Self {
        Self {
            _phantom: PhantomData,
        }
    }
}

impl<R: GraphicsRuntime<GraphicsApi = Software>> fluxemu_runtime::Platform for Platform<R> {
    type GraphicsApi = Software;
}

impl<R: GraphicsRuntime<GraphicsApi = Software>> fluxemu_frontend::Platform for Platform<R> {
    type AudioRuntime = AudioRuntime;
    type GraphicsRuntime = R;

    const EXTERNAL_FILE_DIALOGS_SUPPORTED: bool = false;
}
