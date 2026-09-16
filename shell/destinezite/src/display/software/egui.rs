use fluxemu_frontend::graphics::present_machine_software;
use fluxemu_frontend_egui::rendering::{DrawTarget, EguiCapableGraphicsRuntime};
use fluxemu_graphics::texture::AsViewTextureMut;
use palette::Srgb;

use crate::display::software::{SoftwareCompatibleDisplayContext, SoftwareGraphicsRuntime};

impl<D: SoftwareCompatibleDisplayContext> EguiCapableGraphicsRuntime
    for SoftwareGraphicsRuntime<D>
{
    fn present<'a>(
        &'a mut self,
        clear_color: Srgb<u8>,
        targets: impl IntoIterator<Item = DrawTarget<'a>>,
    ) {
        let mut surface_buffer_guard = self
            .display_handle
            .map_surface_buffer(&mut self.surface)
            .unwrap();

        let mut surface_buffer = surface_buffer_guard.as_view_mut();

        surface_buffer.fill(clear_color.into());

        for target in targets {
            match target {
                DrawTarget::Gui {
                    context,
                    full_output,
                } => {
                    // Benchmarks say that a batch size of 16 is the most ideal across several low and mid power machines
                    //
                    // As far as throughput for realistic ui goes at the very least
                    //
                    // This is suggested by benchmarks on a i5-1245U and a RK3566T
                    self.renderer
                        .render::<_, 16>(context, full_output, &mut surface_buffer);
                }
                DrawTarget::Machine { machine } => {
                    present_machine_software(machine, &mut surface_buffer);
                }
            }
        }

        drop(surface_buffer_guard);

        self.display_handle.pre_present_notify();
        self.display_handle.present(&mut self.surface).unwrap();
    }
}
