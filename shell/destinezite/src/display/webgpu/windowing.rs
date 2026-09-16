use fluxemu_frontend::graphics::ProducableGraphicsRuntime;
use fluxemu_graphics::api::webgpu::Webgpu;
use fluxemu_runtime::graphics::GraphicsRequirements;
use nalgebra::Vector2;
use wgpu::{CreateSurfaceError, Instance, InstanceDescriptor, Surface};

use crate::{
    display::webgpu::{
        ConfigurationDependentData, WebgpuCompatibleDisplayContext, WebgpuGraphicsRuntime,
    },
    event_loop::windowing::Window,
};

impl ProducableGraphicsRuntime<Window> for WebgpuGraphicsRuntime<Window> {
    fn new(window: &Window, graphics_requirements: GraphicsRequirements<Webgpu>) -> Self {
        let inner_size = window.0.inner_size();

        let configuration_dependent_data = ConfigurationDependentData::new(
            Vector2::new(inner_size.width, inner_size.height),
            graphics_requirements,
            window,
        );

        WebgpuGraphicsRuntime {
            display_handle: window.clone(),
            configuration_dependent_data: Some(configuration_dependent_data),
        }
    }

    fn display_context(&self) -> &Window {
        &self.display_handle
    }
}

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
