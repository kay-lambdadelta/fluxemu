use std::fmt::Debug;

use fluxemu_runtime::graphics::{GraphicsApi, software::Texture};
use palette::Srgba;

use crate::ppu::region::Region;

pub mod software;
#[cfg(feature = "webgpu")]
pub mod webgpu;

pub(crate) trait PpuDisplayBackend<R: Region>:
    Send + Sync + Debug + Sized + 'static
{
    type GraphicsApi: GraphicsApi;

    fn new(initialization_data: <Self::GraphicsApi as GraphicsApi>::InitializationData) -> Self;
    fn create_framebuffer(&self) -> <Self::GraphicsApi as GraphicsApi>::Texture;
    fn commit_staging_buffer(
        &mut self,
        staging_buffer: &Texture<Srgba<u8>>,
        framebuffer: &mut <Self::GraphicsApi as GraphicsApi>::Texture,
    );
}

pub(crate) trait SupportedGraphicsApiPpu: GraphicsApi {
    type Backend<R: Region>: PpuDisplayBackend<R, GraphicsApi = Self>;
}
