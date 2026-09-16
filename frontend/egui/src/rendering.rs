use std::sync::Arc;

use egui::{Context, FullOutput};
use fluxemu_frontend::graphics::GraphicsRuntime;
use fluxemu_runtime::machine::Machine;
use palette::Srgb;

#[allow(clippy::large_enum_variant)]
pub enum DrawTarget<'a> {
    Gui {
        context: &'a Context,
        full_output: FullOutput,
    },
    Machine {
        machine: &'a Arc<Machine>,
    },
}

pub trait EguiCapableGraphicsRuntime: GraphicsRuntime {
    /// Draw these items in this order
    fn present<'a>(
        &'a mut self,
        clear_color: Srgb<u8>,
        targets: impl IntoIterator<Item = DrawTarget<'a>>,
    );
}
