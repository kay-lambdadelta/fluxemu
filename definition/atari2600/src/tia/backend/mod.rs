use std::fmt::Debug;

use fluxemu_runtime::graphics::{GraphicsApi, software::Texture};
use palette::Srgba;

use crate::tia::region::Region;

pub mod software;
#[cfg(feature = "webgpu")]
pub mod webgpu;

pub(crate) trait TiaDisplayBackend<R: Region>:
    Send + Sync + Debug + Sized + 'static
{
    type GraphicsApi: GraphicsApi;

    fn new(initialization_data: <Self::GraphicsApi as GraphicsApi>::InitializationData) -> Self;
    fn framebuffer(&self) -> &<Self::GraphicsApi as GraphicsApi>::Framebuffer;
    fn commit_staging_buffer(&mut self, staging_buffer: &Texture<Srgba<u8>>);
}

pub(crate) trait SupportedGraphicsApiTia: GraphicsApi {
    type Backend<R: Region>: TiaDisplayBackend<R, GraphicsApi = Self>;
}
