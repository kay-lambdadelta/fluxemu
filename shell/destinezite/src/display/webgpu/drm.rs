use std::os::fd::AsRawFd;

use fluxemu_frontend_egui::rendering::webgpu::WebgpuCompatibleDisplayContext;
use wgpu::{Backends, Instance, InstanceDescriptor, SurfaceTargetUnsafe};

use crate::event_loop::drm::{DrmContext, mode_refresh_millihertz};

impl WebgpuCompatibleDisplayContext for DrmContext {
    fn produce_instance_and_surface(
        &self,
    ) -> Result<(Instance, wgpu::Surface<'static>), wgpu::CreateSurfaceError> {
        let instance = Instance::new(InstanceDescriptor {
            // Only vulkan is supported
            backends: Backends::VULKAN,
            ..InstanceDescriptor::new_without_display_handle_from_env()
        });

        let (width, height) = self.params.mode.size();
        let refresh_rate = mode_refresh_millihertz(&self.params.mode);
        let plane_id = self.card.find_suitable_plane(self.params.crtc_handle);

        let surface_target_specifier = SurfaceTargetUnsafe::Drm {
            fd: self.card.as_raw_fd(),
            plane: plane_id.into(),
            connector_id: self.params.connector_handle.into(),
            width: width as u32,
            height: height as u32,
            refresh_rate,
        };

        let surface = unsafe { instance.create_surface_unsafe(surface_target_specifier) }?;

        Ok((instance, surface))
    }
}
