use std::fmt::Debug;

use fluxemu_graphics::api::GraphicsApi;
use fluxemu_runtime::graphics::SimpleDisplayBackend;

use crate::ppu::region::Region;

pub mod software;
#[cfg(feature = "webgpu")]
pub mod webgpu;

pub(crate) trait SupportedGraphicsApi: GraphicsApi {
    type Backend<R: Region>: SimpleDisplayBackend<GraphicsApi = Self> + Send + Sync + Debug;
}
