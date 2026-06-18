use std::sync::Arc;

use fluxemu_graphics::api::webgpu::Webgpu;
use fluxemu_runtime::graphics::GraphicsRequirements;
use nalgebra::Vector2;
use wgpu::{CreateSurfaceError, Instance, InstanceDescriptor, Surface};
use winit::window::Window;

use crate::display::{
    RuntimeAssociatedDisplayContext,
    webgpu::{ConfigurationDependentData, WebgpuCompatibleDisplayContext, WebgpuGraphicsRuntime},
};

impl RuntimeAssociatedDisplayContext<WebgpuGraphicsRuntime<Arc<Window>>> for Arc<Window> {
    type ProduceDataArgs<'a> = ();

    fn produce_runtime(
        &self,
        graphics_requirements: GraphicsRequirements<Webgpu>,
        _args: Self::ProduceDataArgs<'_>,
    ) -> WebgpuGraphicsRuntime<Arc<Window>> {
        let inner_size = self.inner_size();

        let configuration_dependent_data = ConfigurationDependentData::new(
            Vector2::new(inner_size.width, inner_size.height),
            graphics_requirements,
            self,
        );

        WebgpuGraphicsRuntime {
            display_handle: self.clone(),
            configuration_dependent_data: Some(configuration_dependent_data),
        }
    }
}

impl WebgpuCompatibleDisplayContext for Arc<Window> {
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
