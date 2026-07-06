use std::fmt::Debug;

use fluxemu_graphics::api::{
    GraphicsApi,
    software::texture::{CopyMode, RefMutTexture, RefTexture},
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
    fn commit_staging_buffer(&mut self, staging_buffer: RefTexture<PpuColorIndex>);
}

pub(crate) trait SupportedGraphicsApiPpu: GraphicsApi {
    type Backend<R: Region>: PpuDisplayBackend<R, GraphicsApi = Self>;
}

#[inline]
fn convert_paletted_staging_buffer<R: Region>(
    staging_buffer: RefTexture<PpuColorIndex>,
    mut framebuffer: RefMutTexture<Srgba<u8>>,
) {
    assert_eq!(staging_buffer.size(), framebuffer.size());

    framebuffer.map_from(
        staging_buffer,
        CopyMode::Nearest,
        #[inline]
        |index| {
            let clamped_index = (index as usize).min(R::COLOR_PALETTE.len() - 1);

            R::COLOR_PALETTE[clamped_index].into()
        },
    );
}
