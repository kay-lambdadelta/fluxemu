use std::{any::Any, borrow::Cow, marker::PhantomData, ops::RangeInclusive, sync::Arc};

use bytes::Bytes;
use fluxemu_input::InputId;
use fluxemu_program::{ProgramManager, RomId};

use crate::{
    component::{Component, config::ComponentConfig},
    event::EventMode,
    graphics::GraphicsRequirements,
    input::{LogicalInputDevice, LogicalInputDeviceMetadata},
    machine::builder::{
        ComponentLateInitializer, MachineBuilder, RomRequirement, SchedulerParticipation,
    },
    memory::{Address, AddressSpaceId, MapTarget, MemoryRemappingCommand, Permissions},
    path::{ComponentPath, ResourcePath},
    persistence::{Codec, ErasedCodec, ErasedCodecWrapper},
    platform::Platform,
    scheduler::Period,
};

/// Overall data extracted from components needed for machine initialization
pub(super) struct ComponentData<P: Platform> {
    pub late_initializer: ComponentLateInitializer<P>,
    pub save_codec: Option<Box<dyn ErasedCodec>>,
    pub snapshot_codec: Option<Box<dyn ErasedCodec>>,
    pub graphics_requirements: GraphicsRequirements<P::GraphicsApi>,
    pub scheduler_participation: Option<SchedulerParticipation>,
}

impl<P: Platform> ComponentData<P> {
    pub fn new<B: ComponentConfig<P>>() -> Self {
        Self {
            late_initializer: Box::new(|component, data| {
                let component: &mut B::Component =
                    (component as &mut dyn Any).downcast_mut().unwrap();

                B::late_initialize(component, data)
            }),
            save_codec: None,
            snapshot_codec: None,
            graphics_requirements: GraphicsRequirements::default(),
            scheduler_participation: None,
        }
    }
}

pub struct ComponentBuilder<'a, P: Platform, C: Component> {
    pub(super) machine_builder: &'a mut MachineBuilder<P>,
    pub(super) component_data: &'a mut ComponentData<P>,
    pub(super) path: &'a ComponentPath,
    pub(super) _phantom: PhantomData<C>,
}

impl<P: Platform, C: Component> ComponentBuilder<'_, P, C> {
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

    pub fn scheduler_participation(
        self,
        scheduler_participation: Option<SchedulerParticipation>,
    ) -> Self {
        if scheduler_participation == Some(SchedulerParticipation::SchedulerDriven) {
            self.machine_builder
                .scheduler
                .register_driven_component(self.path.clone());
        }
        self.component_data.scheduler_participation = scheduler_participation;

        self
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

    /// Insert a callback into the memory translation table for reading
    pub fn memory_map_component_read(
        self,
        address_space: AddressSpaceId,
        range: RangeInclusive<Address>,
    ) -> Self {
        self.machine_builder
            .address_spaces
            .get_mut(&address_space)
            .unwrap()
            .commands
            .push(MemoryRemappingCommand::Map {
                range,
                target: MapTarget::Component(self.path.clone()),
                permissions: Permissions::READ,
            });

        self
    }

    pub fn memory_map_component_write(
        self,
        address_space: AddressSpaceId,
        range: RangeInclusive<Address>,
    ) -> Self {
        self.machine_builder
            .address_spaces
            .get_mut(&address_space)
            .unwrap()
            .commands
            .push(MemoryRemappingCommand::Map {
                range,
                target: MapTarget::Component(self.path.clone()),
                permissions: Permissions::WRITE,
            });

        self
    }

    pub fn memory_map_component(
        self,
        address_space: AddressSpaceId,
        range: RangeInclusive<Address>,
    ) -> Self {
        self.machine_builder
            .address_spaces
            .get_mut(&address_space)
            .unwrap()
            .commands
            .push(MemoryRemappingCommand::Map {
                range,
                target: MapTarget::Component(self.path.clone()),
                permissions: Permissions::ALL,
            });

        self
    }

    pub fn memory_mirror_map_read(
        self,
        address_space: AddressSpaceId,
        source: RangeInclusive<Address>,
        destination: RangeInclusive<Address>,
    ) -> Self {
        self.machine_builder
            .address_spaces
            .get_mut(&address_space)
            .unwrap()
            .commands
            .push(MemoryRemappingCommand::Map {
                range: source,
                target: MapTarget::Mirror { destination },
                permissions: Permissions::READ,
            });

        self
    }

    pub fn memory_mirror_map_write(
        self,
        address_space: AddressSpaceId,
        source: RangeInclusive<Address>,
        destination: RangeInclusive<Address>,
    ) -> Self {
        self.machine_builder
            .address_spaces
            .get_mut(&address_space)
            .unwrap()
            .commands
            .push(MemoryRemappingCommand::Map {
                range: source,
                target: MapTarget::Mirror { destination },
                permissions: Permissions::WRITE,
            });

        self
    }

    pub fn memory_mirror_map(
        self,
        address_space: AddressSpaceId,
        source: RangeInclusive<Address>,
        destination: RangeInclusive<Address>,
    ) -> Self {
        self.machine_builder
            .address_spaces
            .get_mut(&address_space)
            .unwrap()
            .commands
            .push(MemoryRemappingCommand::Map {
                range: source,
                target: MapTarget::Mirror { destination },
                permissions: Permissions::ALL,
            });

        self
    }

    pub fn memory_map_buffer_read(
        self,
        address_space: AddressSpaceId,
        range: RangeInclusive<Address>,
        memory: impl Into<Bytes>,
    ) -> Self {
        self.machine_builder
            .address_spaces
            .get_mut(&address_space)
            .unwrap()
            .commands
            .push(MemoryRemappingCommand::Map {
                range,
                target: MapTarget::Buffer(memory.into()),
                permissions: Permissions::READ,
            });

        self
    }

    pub fn schedule_event<C2: Component>(
        self,
        target_path: &ComponentPath,
        time: Period,
        requeue_mode: EventMode,
        data: C2::Event,
    ) -> Self {
        self.machine_builder.scheduler.event_manager.schedule(
            time,
            target_path.clone(),
            requeue_mode,
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

    pub fn save_codec<CO: Codec<Component = C>>(self, codec: CO) -> Self {
        let erased = ErasedCodecWrapper::new(codec);

        self.component_data.save_codec = Some(Box::new(erased));

        self
    }

    pub fn snapshot_codec<CO: Codec<Component = C>>(self, codec: CO) -> Self {
        let erased = ErasedCodecWrapper::new(codec);

        self.component_data.snapshot_codec = Some(Box::new(erased));

        self
    }
}
