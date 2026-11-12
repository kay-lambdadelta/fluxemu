use fluxemu_frontend_egui::rendering::webgpu::WebgpuCompatibleDisplayContext;
use wgpu::{CreateSurfaceError, Instance, InstanceDescriptor, Surface};

use crate::event_loop::windowing::Window;

impl WebgpuCompatibleDisplayContext for Window {
    fn produce_instance_and_surface(
        &self,
    ) -> Result<(Instance, Surface<'static>), CreateSurfaceError> {
        let instance = Instance::new(InstanceDescriptor::new_with_display_handle_from_env(
            Box::new(self.clone()),
        ));

        let surface = instance.create_surface(self.clone())?;

        Ok((instance, surface))
    }
}
