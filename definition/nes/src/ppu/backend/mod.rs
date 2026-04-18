use std::fmt::Debug;

use fluxemu_runtime::graphics::{
    GraphicsApi,
    software::{Texture, TextureImpl},
};
use palette::Srgba;

use crate::ppu::{color::PpuColorIndex, region::Region};

pub mod software;
#[cfg(feature = "webgpu")]
pub mod webgpu;

pub(crate) trait PpuDisplayBackend<R: Region>:
    Send + Sync + Debug + Sized + 'static
{
    type GraphicsApi: GraphicsApi;

    fn new(initialization_data: <Self::GraphicsApi as GraphicsApi>::InitializationData) -> Self;
    fn framebuffer(&self) -> &<Self::GraphicsApi as GraphicsApi>::Framebuffer;
    fn commit_staging_buffer(&mut self, staging_buffer: &Texture<PpuColorIndex>);
}

pub(crate) trait SupportedGraphicsApiPpu: GraphicsApi {
    type Backend<R: Region>: PpuDisplayBackend<R, GraphicsApi = Self>;
}

fn convert_paletted_staging_buffer<R: Region>(
    staging_buffer: &Texture<u8>,
    framebuffer: &mut Texture<Srgba<u8>>,
) {
    staging_buffer.iter_pixels(|point, index| {
        let color = R::COLOR_PALETTE[*index as usize];

        framebuffer[point] = color.into();
    });
}
