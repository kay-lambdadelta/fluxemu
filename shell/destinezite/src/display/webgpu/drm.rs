use std::os::fd::AsRawFd;

use fluxemu_frontend::graphics::ProducableGraphicsRuntime;
use fluxemu_graphics::api::webgpu::Webgpu;
use fluxemu_runtime::graphics::GraphicsRequirements;
use nalgebra::Vector2;
use wgpu::{Backends, Instance, InstanceDescriptor, SurfaceTargetUnsafe};

use crate::{
    display::webgpu::{
        ConfigurationDependentData, WebgpuCompatibleDisplayContext, WebgpuGraphicsRuntime,
    },
    event_loop::drm::{DrmContext, mode_refresh_millihertz},
};

impl ProducableGraphicsRuntime<DrmContext> for WebgpuGraphicsRuntime<DrmContext> {
    fn new(
        context: &DrmContext,
        graphics_requirements: GraphicsRequirements<Webgpu>,
    ) -> WebgpuGraphicsRuntime<DrmContext> {
        let (width, height) = context.params.mode.size();

        let configuration_dependent_data = ConfigurationDependentData::new(
            Vector2::new(width as u32, height as u32),
            graphics_requirements,
            context,
        );

        WebgpuGraphicsRuntime {
            display_handle: context.clone(),
            configuration_dependent_data: Some(configuration_dependent_data),
        }
    }

    fn display_context(&self) -> &DrmContext {
        &self.display_handle
    }
}

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
