use std::fmt::Display;

use fluxemu_graphics::api::GraphicsApi;
use fluxemu_runtime::graphics::{DisplayBackend, DisplayBackendSelector, Submitter};

use crate::tia::region::Region;

pub mod software;
#[cfg(feature = "webgpu")]
pub mod webgpu;

pub(crate) fn display_backend<R: Region, G: GraphicsApi>(
    submitter: Submitter<G>,
    component: impl Display,
) -> DisplayBackend<G> {
    let selector = DisplayBackendSelector::new(submitter);

    #[cfg(feature = "webgpu")]
    let selector = selector.with::<fluxemu_graphics::api::webgpu::Webgpu>(|submitter| {
        Box::new(webgpu::State::<R>::new(submitter))
    });

    selector.or_software(component, |submitter| {
        Box::new(software::State::<R>::new(submitter))
    })
}
