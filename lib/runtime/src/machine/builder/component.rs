use std::{
    any::Any, borrow::Cow, collections::HashMap, marker::PhantomData, ops::RangeInclusive,
    sync::Arc,
};

use bytes::Bytes;
use fluxemu_input::InputId;
use fluxemu_program::{ProgramManager, RomId};

use crate::{
    component::{Component, TaskEntry, config::ComponentConfig},
    graphics::GraphicsRequirements,
    input::{LogicalInputDevice, LogicalInputDeviceMetadata},
    machine::builder::{ComponentLateInitializer, MachineBuilder, RomRequirement},
    memory::{AddressSpaceId, MemoryMapCommand, RegionInitializationData},
    path::{ComponentPath, ResourcePath},
    platform::Platform,
    scheduler::{
        Period,
        event::EventMode,
        task::{self, Mode, Task},
    },
};

/// Overall data extracted from components needed for machine initialization
pub(super) struct ComponentData<P: Platform> {
    pub late_initializer: ComponentLateInitializer<P>,
    pub graphics_requirements: GraphicsRequirements<P::GraphicsApi>,
    pub systems: HashMap<Cow<'static, str>, TaskEntry>,
}

impl<P: Platform> ComponentData<P> {
    pub fn new<B: ComponentConfig<P>>() -> Self {
        Self {
            late_initializer: Box::new(|component, data| {
                let component: &mut B::Component =
                    (component as &mut dyn Any).downcast_mut().unwrap();

                B::late_initialize(component, data)
            }),
            graphics_requirements: GraphicsRequirements::default(),
            systems: HashMap::new(),
        }
    }
}

/// Builder for a component
///
/// This is used as a way for components to construct themselves
pub struct ComponentBuilder<'a, P: Platform, C: Component> {
    pub(super) machine_builder: &'a mut MachineBuilder<P>,
    pub(super) component_data: &'a mut ComponentData<P>,
    pub(super) path: &'a ComponentPath,
    pub(super) _phantom: PhantomData<C>,
}

impl<P: Platform, C: Component> ComponentBuilder<'_, P, C> {
    /// Resulting path of the component
    pub fn path(&self) -> &ComponentPath {
        self.path
    }

    pub fn open_rom(
        &self,
        id: RomId,
        requirement: RomRequirement,
    ) -> Result<Option<Bytes>, fluxemu_program::Error> {
        self.machine_builder.open_rom(id, requirement)
    }

    pub fn program_manager(&self) -> &ProgramManager {
        self.machine_builder.program_manager()
    }

    /// Insert a component into the machine
    pub fn component<B: ComponentConfig<P>>(
        self,
        name: impl Into<Cow<'static, str>>,
        config: B,
    ) -> (Self, ComponentPath) {
        let component_path = self.path.clone();
        let component_path = component_path.join(&name.into()).unwrap();

        self.machine_builder
            .insert_component_with_path(component_path.clone(), config);

        (self, component_path)
    }

    /// Insert a component with a default config
    pub fn default_component<B: ComponentConfig<P> + Default>(
        self,
        name: impl Into<Cow<'static, str>>,
    ) -> (Self, ComponentPath) {
        let config = B::default();

        self.component(name, config)
    }

    pub fn audio_channel(self, name: impl Into<Cow<'static, str>>) -> (Self, ResourcePath) {
        let resource_path = self.path.clone().into_resource(name).unwrap();

        self.machine_builder
            .audio_channels
            .insert(resource_path.clone());

        (self, resource_path)
    }

    pub fn framebuffer(self, name: impl Into<Cow<'static, str>>) -> (Self, ResourcePath) {
        let resource_path = self.path.clone().into_resource(name).unwrap();

        self.machine_builder
            .framebuffers
            .insert(resource_path.clone());

        (self, resource_path)
    }

    /// Create a input device resource that this component owns
    ///
    /// Note that this also gives the component wake up events for relevant input changes
    pub fn input(
        self,
        name: impl Into<Cow<'static, str>>,
        present_inputs: impl IntoIterator<Item = InputId>,
        default_mappings: impl IntoIterator<Item = (InputId, InputId)>,
    ) -> (Self, Arc<LogicalInputDevice>) {
        let resource_path = self.path.clone().into_resource(name).unwrap();

        let device = Arc::new(LogicalInputDevice::new(LogicalInputDeviceMetadata {
            path: resource_path.clone(),
            present_inputs: present_inputs.into_iter().collect(),
            default_mappings: default_mappings.into_iter().collect(),
        }));

        self.machine_builder
            .input_devices
            .insert(resource_path, device.clone());

        (self, device)
    }

    /// Creates a mutable memory region
    pub fn memory(
        self,
        name: impl Into<Cow<'static, str>>,
        size: usize,
        initial_contents: impl IntoIterator<Item = (RangeInclusive<usize>, Bytes)>,
    ) -> (Self, ResourcePath) {
        let path = ResourcePath::new(None, name).unwrap();

        self.machine_builder.required_memory_regions.insert(
            path.clone(),
            RegionInitializationData {
                size,
                sram: false,
                initial_contents: initial_contents.into_iter().collect(),
            },
        );

        (self, path)
    }

    /// Creates a mutable memory region that will be committed to saves
    pub fn save_memory(
        self,
        name: impl Into<Cow<'static, str>>,
        size: usize,
        initial_contents: impl IntoIterator<Item = (RangeInclusive<usize>, Bytes)>,
    ) -> (Self, ResourcePath) {
        let path = ResourcePath::new(None, name).unwrap();

        self.machine_builder.required_memory_regions.insert(
            path.clone(),
            RegionInitializationData {
                size,
                sram: true,
                initial_contents: initial_contents.into_iter().collect(),
            },
        );

        (self, path)
    }

    pub fn map_memory(
        self,
        address_space: AddressSpaceId,
        commands: impl IntoIterator<Item = MemoryMapCommand>,
    ) -> Self {
        self.machine_builder
            .address_spaces
            .get_mut(&address_space)
            .unwrap()
            .commands
            .extend(commands);

        self
    }

    pub fn schedule_event<C2: Component>(
        self,
        target_path: &ComponentPath,
        time: Period,
        event_mode: EventMode,
        data: C2::Event,
    ) -> Self {
        self.machine_builder.scheduler.queue.schedule_event(
            target_path.clone(),
            time,
            event_mode,
            Box::new(data),
        );

        self
    }

    pub fn add_graphics_requirements(
        self,
        requirements: GraphicsRequirements<P::GraphicsApi>,
    ) -> Self {
        self.component_data.graphics_requirements =
            self.component_data.graphics_requirements.clone() | requirements;

        self
    }

    pub fn task(
        self,
        name: impl Into<Cow<'static, str>>,
        mode: task::Mode,
        system: impl Task<Component = C>,
    ) -> Self {
        let name = name.into();

        if mode == Mode::Always {
            let path = self.path.clone().into_resource(name.clone()).unwrap();

            self.machine_builder
                .scheduler
                .queue
                .reschedule_task(path, Period::ZERO);
        }

        self.component_data
            .systems
            .insert(name, TaskEntry::new(mode, system));

        self
    }
}
