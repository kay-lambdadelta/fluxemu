use std::{
    any::Any, borrow::Cow, collections::HashSet, io::Read, marker::PhantomData,
    ops::RangeInclusive, sync::Arc,
};

use bytes::Bytes;
use fluxemu_input::InputId;
use fluxemu_program::{ProgramManager, RomId};

use crate::{
    component::{Component, config::ComponentConfig},
    event::EventMode,
    graphics::GraphicsRequirements,
    input::{LogicalInputDevice, LogicalInputDeviceMetadata},
    machine::builder::{
        ComponentLateInitializer, MachineBuilder, MachineBuilderCommand, RomRequirement,
        SchedulerParticipation,
    },
    memory::{Address, AddressSpaceId, MapTarget, MemoryRemappingCommand, Permissions},
    path::{ComponentPath, ResourcePath},
    persistence::PersistanceFormatVersion,
    platform::Platform,
    scheduler::Period,
};

/// Overall data extracted from components needed for machine initialization
pub(super) struct ComponentData<'a, P: Platform> {
    pub audio_outputs: HashSet<ResourcePath>,
    pub late_initializer: ComponentLateInitializer<P>,
    pub scheduler_participation: Option<SchedulerParticipation>,
    pub save_version: Option<PersistanceFormatVersion>,
    pub snapshot_version: PersistanceFormatVersion,
    pub local_commands: Vec<MachineBuilderCommand<'a, P>>,
}

impl<P: Platform> ComponentData<'_, P> {
    pub fn new<B: ComponentConfig<P>>() -> Self {
        Self {
            audio_outputs: HashSet::default(),
            late_initializer: Box::new(|component, data| {
                let component: &mut B::Component =
                    (component as &mut dyn Any).downcast_mut().unwrap();

                B::late_initialize(component, data)
            }),
            scheduler_participation: None,
            save_version: None,
            snapshot_version: B::CURRENT_SNAPSHOT_VERSION,
            local_commands: Vec::default(),
        }
    }
}

pub struct ComponentBuilder<'a, 'b, P: Platform, C: Component> {
    pub(super) machine_builder: &'a mut MachineBuilder<'b, P>,
    pub(super) component_data: &'a mut ComponentData<'b, P>,
    pub(super) path: &'a ComponentPath,
    pub(super) _phantom: PhantomData<C>,
}

impl<'b, P: Platform, C: Component> ComponentBuilder<'_, 'b, P, C> {
    pub fn path(&self) -> &ComponentPath {
        self.path
    }

    pub fn get_save(&self) -> Option<(impl Read, PersistanceFormatVersion)> {
        None::<(&[u8], _)>
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

    /// Register that this component will participate in saves
    pub fn save_version(self, version: PersistanceFormatVersion) -> Self {
        self.component_data.save_version = Some(version);

        self
    }

    pub fn scheduler_participation(
        self,
        scheduler_participation: Option<SchedulerParticipation>,
    ) -> Self {
        self.component_data.scheduler_participation = scheduler_participation;

        self
    }

    /// Insert a component into the machine
    pub fn component<B: ComponentConfig<P> + 'b>(
        self,
        name: impl Into<Cow<'static, str>>,
        config: B,
    ) -> (Self, ComponentPath) {
        let component_path = self.path.clone();
        let component_path = component_path.join(&name.into()).unwrap();

        let command = MachineBuilder::insert_component_with_path(component_path.clone(), config);

        self.component_data.local_commands.push(command);

        (self, component_path)
    }

    /// Insert a component with a default config
    pub fn default_component<B: ComponentConfig<P> + Default + 'b>(
        self,
        name: impl Into<Cow<'static, str>>,
    ) -> (Self, ComponentPath) {
        let config = B::default();

        self.component(name, config)
    }

    pub fn audio_channel(self, name: impl Into<Cow<'static, str>>) -> (Self, ResourcePath) {
        let resource_path = self.path.clone().into_resource(name).unwrap();

        self.component_data
            .audio_outputs
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
            path: resource_path,
            present_inputs: present_inputs.into_iter().collect(),
            default_mappings: default_mappings.into_iter().collect(),
        }));

        self.component_data
            .local_commands
            .push(MachineBuilderCommand::CreateInputDevice(device.clone()));

        (self, device)
    }

    /// Insert a callback into the memory translation table for reading
    pub fn memory_map_component_read(
        self,
        address_space: AddressSpaceId,
        range: RangeInclusive<Address>,
    ) -> Self {
        self.component_data
            .local_commands
            .push(MachineBuilderCommand::MemoryMap {
                address_space,
                command: MemoryRemappingCommand::Map {
                    range,
                    target: MapTarget::Component(self.path.clone()),
                    permissions: Permissions::READ,
                },
            });

        self
    }

    pub fn memory_map_component_write(
        self,
        address_space: AddressSpaceId,
        range: RangeInclusive<Address>,
    ) -> Self {
        self.component_data
            .local_commands
            .push(MachineBuilderCommand::MemoryMap {
                address_space,
                command: MemoryRemappingCommand::Map {
                    range,
                    target: MapTarget::Component(self.path.clone()),
                    permissions: Permissions::WRITE,
                },
            });

        self
    }

    pub fn memory_map_component(
        self,
        address_space: AddressSpaceId,
        range: RangeInclusive<Address>,
    ) -> Self {
        self.component_data
            .local_commands
            .push(MachineBuilderCommand::MemoryMap {
                address_space,
                command: MemoryRemappingCommand::Map {
                    range,
                    target: MapTarget::Component(self.path.clone()),
                    permissions: Permissions::ALL,
                },
            });

        self
    }

    pub fn memory_mirror_map_read(
        self,
        address_space: AddressSpaceId,
        source: RangeInclusive<Address>,
        destination: RangeInclusive<Address>,
    ) -> Self {
        self.component_data
            .local_commands
            .push(MachineBuilderCommand::MemoryMap {
                address_space,
                command: MemoryRemappingCommand::Map {
                    range: source,
                    target: MapTarget::Mirror { destination },
                    permissions: Permissions::READ,
                },
            });

        self
    }

    pub fn memory_mirror_map_write(
        self,
        address_space: AddressSpaceId,
        source: RangeInclusive<Address>,
        destination: RangeInclusive<Address>,
    ) -> Self {
        self.component_data
            .local_commands
            .push(MachineBuilderCommand::MemoryMap {
                address_space,
                command: MemoryRemappingCommand::Map {
                    range: source,
                    target: MapTarget::Mirror { destination },
                    permissions: Permissions::WRITE,
                },
            });

        self
    }

    pub fn memory_mirror_map(
        self,
        address_space: AddressSpaceId,
        source: RangeInclusive<Address>,
        destination: RangeInclusive<Address>,
    ) -> Self {
        self.component_data
            .local_commands
            .push(MachineBuilderCommand::MemoryMap {
                address_space,
                command: MemoryRemappingCommand::Map {
                    range: source,
                    target: MapTarget::Mirror { destination },
                    permissions: Permissions::ALL,
                },
            });

        self
    }

    pub fn memory_map_buffer_read(
        self,
        address_space: AddressSpaceId,
        range: RangeInclusive<Address>,
        memory: impl Into<Bytes>,
    ) -> Self {
        self.component_data
            .local_commands
            .push(MachineBuilderCommand::MemoryMap {
                address_space,
                command: MemoryRemappingCommand::Map {
                    range,
                    target: MapTarget::Buffer(memory.into()),
                    permissions: Permissions::READ,
                },
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
        self.component_data
            .local_commands
            .push(MachineBuilderCommand::InsertEvent {
                data: Box::new(data),
                requeue_mode,
                time,
                path: target_path.clone(),
            });

        self
    }

    pub fn add_graphics_requirements(
        self,
        requirements: GraphicsRequirements<P::GraphicsApi>,
    ) -> Self {
        self.component_data
            .local_commands
            .push(MachineBuilderCommand::AddGraphicsRequirements { requirements });

        self
    }
}
