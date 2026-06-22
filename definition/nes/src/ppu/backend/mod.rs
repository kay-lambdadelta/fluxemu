use std::fmt::Debug;

use fluxemu_graphics::api::{
    GraphicsApi,
    software::texture::{OwnedTexture, RefMutTexture},
};
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
    fn framebuffer(&self) -> &<Self::GraphicsApi as GraphicsApi>::Framebuffer;
    fn commit_staging_buffer(&mut self, staging_buffer: &OwnedTexture<u8>);
}

pub(crate) trait SupportedGraphicsApiPpu: GraphicsApi {
    type Backend<R: Region>: PpuDisplayBackend<R, GraphicsApi = Self>;
}

fn convert_paletted_staging_buffer<R: Region>(
    staging_buffer: &OwnedTexture<u8>,
    mut framebuffer: RefMutTexture<Srgba<u8>>,
) {
    for (point, index) in staging_buffer.iter_pixels_indexed() {
        let color = R::COLOR_PALETTE[*index as usize];

        framebuffer[point] = color.into();
    }
}
