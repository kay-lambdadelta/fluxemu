use std::{borrow::Cow, collections::HashMap, ops::DerefMut, sync::Arc};

use crate::{
    component::{Component, LateContext, LateInitializedData},
    graphics::GraphicsApi,
    input::LogicalInputDevice,
    machine::{Machine, graphics::GraphicsRequirements, registry::ComponentRegistry},
    memory::{AddressSpaceId, MemoryRemappingCommand},
    path::{ComponentPath, ResourcePath},
    platform::Platform,
    scheduler::{EventType, Period},
};

mod component;
mod machine;

pub use component::*;
pub use machine::*;

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Clone, Copy)]
pub enum SchedulerParticipation {
    /// The scheduler will make no attempt to time synchronize this component
    None,
    /// [`crate::component::Component::synchronize`] will only be called upon interaction
    OnAccess,
    /// [`crate::component::Component::synchronize`] will also be called when the scheduler advances time
    SchedulerDriven,
}

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

struct PartialSyncPoint {
    ty: EventType,
    time: Period,
    name: Cow<'static, str>,
}

#[allow(type_alias_bounds)]
type ComponentConstructor<'a, P: Platform> = Box<
    dyn for<'b> FnOnce(
            &'b mut MachineBuilder<'a, P>,
            &'b mut ComponentRegistry,
        ) -> ComponentData<'a, P>
        + Sync
        + Send
        + 'a,
>;

enum MachineBuilderCommand<'a, P: Platform> {
    CreateComponent {
        constructor: ComponentConstructor<'a, P>,
        path: ComponentPath,
    },
    CreateAddressSpace {
        id: AddressSpaceId,
        width: u8,
    },
    MemoryMap {
        address_space: AddressSpaceId,
        command: MemoryRemappingCommand,
    },
    CreateFramebuffer {
        path: ResourcePath,
    },
    CreateInputDevice(Arc<LogicalInputDevice>),
    AddGraphicsRequirements {
        requirements: GraphicsRequirements<P::GraphicsApi>,
    },
}

type ComponentLateInitializer<P> =
    Box<dyn FnOnce(&mut dyn Component, &LateContext<P>) -> LateInitializedData<P> + Send + Sync>;

pub struct SealedMachineBuilder<P: Platform> {
    machine: Arc<Machine>,
    #[allow(clippy::type_complexity)]
    component_late_initializers: HashMap<ComponentPath, ComponentLateInitializer<P>>,
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
            machine: self.machine.clone(),
        };

        for (path, initializer) in self.component_late_initializers.drain() {
            self.machine
                .registry
                .interact_dyn_mut(&path, Period::ZERO, |mut component| {
                    let provided_data = initializer(component.deref_mut(), &late_initialized_data);

                    for (framebuffer_name, framebuffer) in provided_data.framebuffers {
                        let framebuffer_path = path
                            .clone()
                            .into_resource(framebuffer_name)
                            .expect("Invalid framebuffer name");

                        // Replace dummy type with actual framebuffer
                        *self
                            .machine
                            .framebuffers
                            .get(&framebuffer_path)
                            .unwrap()
                            .lock()
                            .unwrap() = Box::new(framebuffer);
                    }
                })
                .unwrap();
        }

        self.machine
    }
}

/// Helper trait representing a fully constructed machine
pub trait MachineFactory<P: Platform>: Send + Sync + 'static {
    /// Construct a new machine given the parameters
    fn construct<'a>(&self, machine_builder: MachineBuilder<'a, P>) -> MachineBuilder<'a, P>;
}
