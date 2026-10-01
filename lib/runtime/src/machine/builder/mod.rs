use std::{collections::HashMap, ops::DerefMut, sync::Arc};

use crate::{
    ResourcePath,
    component::{Component, config::LateContext},
    graphics::GraphicsRequirements,
    machine::Machine,
    path::ComponentPath,
    platform::Platform,
    scheduler::Period,
};

mod component;
mod machine;

pub use component::*;
use fluxemu_graphics::api::GraphicsApi;
pub use machine::*;
use rustc_hash::FxBuildHasher;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
/// The requirement of a ROM as pertains to a component attempting to load it
pub enum RomRequirement {
    /// Ok to boot machine without this ROM but runtime failure can occur without it
    Sometimes,
    /// Machine will boot emulating this ROM
    Optional,
    /// Machine can not boot without this ROM
    Required,
}

type ComponentLateInitializer<P> =
    Box<dyn FnOnce(&mut dyn Component, &LateContext<P>) + Send + Sync>;

#[allow(type_alias_bounds)]
type FramebufferLateInitializer<P: Platform> = Box<
    dyn FnOnce(&mut dyn Component, &ResourcePath) -> <P::GraphicsApi as GraphicsApi>::Framebuffer
        + Send
        + Sync,
>;

pub struct SealedMachineBuilder<P: Platform> {
    machine: Arc<Machine>,
    #[allow(clippy::type_complexity)]
    component_late_initializers: HashMap<ComponentPath, ComponentLateInitializer<P>, FxBuildHasher>,
    framebuffer_late_initializers:
        HashMap<ResourcePath, FramebufferLateInitializer<P>, FxBuildHasher>,
    graphics_requirements: GraphicsRequirements<P::GraphicsApi>,
}

impl<P: Platform> SealedMachineBuilder<P> {
    pub fn graphics_requirements(&self) -> GraphicsRequirements<P::GraphicsApi> {
        self.graphics_requirements.clone()
    }

    pub fn build(
        mut self,
        graphics_initialization_data: <P::GraphicsApi as GraphicsApi>::InitializationData,
    ) -> Arc<Machine> {
        let late_initialized_data = LateContext {
            graphics_initialization_data,
        };

        let runtime_guard = self.machine.enter_runtime();

        for (path, initializer) in self.component_late_initializers.drain() {
            runtime_guard
                .component_registry()
                .interact_dyn(&path, &Period::ZERO, |mut component| {
                    initializer(component.deref_mut(), &late_initialized_data);
                })
                .unwrap();
        }

        for (path, initializer) in self.framebuffer_late_initializers.drain() {
            let framebuffer = runtime_guard
                .component_registry()
                .interact_dyn(path.parent().unwrap(), &Period::ZERO, |mut component| {
                    initializer(component.deref_mut(), &path)
                })
                .unwrap();

            // Replace the dummy value with the real framebuffer
            *self
                .machine
                .framebuffers
                .get(&path)
                .unwrap()
                .lock()
                .unwrap() = Box::new(framebuffer) as Box<_>;
        }

        drop(runtime_guard);

        self.machine
    }
}
