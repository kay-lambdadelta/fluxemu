use std::{
    any::Any, borrow::Cow, collections::HashSet, io::Read, marker::PhantomData,
    ops::RangeInclusive, sync::Arc,
};

use bytes::Bytes;
use fluxemu_input::InputId;
use fluxemu_program::{ProgramManager, RomId};

use crate::{
    component::{Component, ComponentConfig, ComponentVersion, TypedComponentHandle},
    input::{LogicalInputDevice, LogicalInputDeviceMetadata},
    machine::{
        builder::{
            ComponentLateInitializer, MachineBuilder, MachineBuilderCommand, PartialSyncPoint,
            RomRequirement, SchedulerParticipation,
        },
        graphics::GraphicsRequirements,
        registry::ComponentRegistry,
    },
    memory::{Address, AddressSpaceId, MapTarget, MemoryRemappingCommand, Permissions},
    path::{ComponentPath, ResourcePath},
    platform::Platform,
    scheduler::{EventType, Frequency, Period, PreemptionSignal},
};

/// Overall data extracted from components needed for machine initialization
pub(super) struct ComponentData<'a, P: Platform> {
    pub audio_outputs: HashSet<ResourcePath>,
    pub late_initializer: ComponentLateInitializer<P>,
    pub scheduler_participation: SchedulerParticipation,
    pub sync_points: Vec<PartialSyncPoint>,
    pub preemption_signal: Arc<PreemptionSignal>,
    pub save_version: Option<ComponentVersion>,
    pub snapshot_version: Option<ComponentVersion>,
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
            scheduler_participation: SchedulerParticipation::None,
            sync_points: Vec::default(),
            preemption_signal: Arc::default(),
            save_version: None,
            snapshot_version: None,
            local_commands: Vec::default(),
        }
    }
}

pub struct ComponentBuilder<'a, 'b, P: Platform, C: Component> {
    pub(super) machine_builder: &'a mut MachineBuilder<'b, P>,
    pub(super) component_data: &'a mut ComponentData<'b, P>,
    pub(super) registry: &'a ComponentRegistry,
    pub(super) path: &'a ComponentPath,
    pub(super) _phantom: PhantomData<C>,
}

impl<'b, P: Platform, C: Component> ComponentBuilder<'_, 'b, P, C> {
    pub fn path(&self) -> &ComponentPath {
        self.path
    }

    pub fn get_save(&self) -> Option<(impl Read, ComponentVersion)> {
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

    pub fn save_version(self, version: ComponentVersion) {
        self.component_data.save_version = Some(version);
    }

    pub fn snapshot_version(self, version: ComponentVersion) {
        self.component_data.snapshot_version = Some(version);
    }

    pub fn scheduler_participation(self, scheduler_participation: SchedulerParticipation) -> Self {
        self.component_data.scheduler_participation = scheduler_participation;

        self
    }

    /// Insert a component into the machine
    pub fn component<B: ComponentConfig<P> + 'b>(
        self,
        name: impl Into<Cow<'static, str>>,
        config: B,
    ) -> (Self, ComponentPath) {
        let mut component_path = self.path.clone();
        component_path.push(&name.into()).unwrap();

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

        self.component_data
            .local_commands
            .push(MachineBuilderCommand::CreateFramebuffer {
                path: resource_path.clone(),
            });

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
                    permissions: Permissions {
                        read: true,
                        write: false,
                    },
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
                    permissions: Permissions {
                        read: false,
                        write: true,
                    },
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
                    permissions: Permissions {
                        read: true,
                        write: true,
                    },
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
                    permissions: Permissions {
                        read: true,
                        write: false,
                    },
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
                    permissions: Permissions {
                        read: false,
                        write: true,
                    },
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
                    permissions: Permissions {
                        read: true,
                        write: true,
                    },
                },
            });

        self
    }

    pub fn memory_register_buffer(
        self,
        address_space: AddressSpaceId,
        name: impl Into<Cow<'static, str>>,
        buffer: Bytes,
    ) -> (Self, ResourcePath) {
        let resource_path = self.path.clone().into_resource(name).unwrap();

        self.component_data
            .local_commands
            .push(MachineBuilderCommand::MemoryMap {
                address_space,
                command: MemoryRemappingCommand::Register {
                    path: resource_path.clone(),
                    buffer,
                },
            });

        (self, resource_path)
    }

    pub fn memory_map_buffer_read(
        self,
        address_space: AddressSpaceId,
        range: RangeInclusive<Address>,
        path: &ResourcePath,
    ) -> Self {
        self.component_data
            .local_commands
            .push(MachineBuilderCommand::MemoryMap {
                address_space,
                command: MemoryRemappingCommand::Map {
                    range,
                    target: MapTarget::Memory(path.clone()),
                    permissions: Permissions {
                        read: true,
                        write: false,
                    },
                },
            });

        self
    }

    pub fn insert_sync_point(self, time: Period, name: impl Into<Cow<'static, str>>) -> Self {
        self.component_data.sync_points.push(PartialSyncPoint {
            ty: EventType::Once,
            time,
            name: name.into(),
        });

        self
    }

    pub fn insert_sync_point_with_frequency(
        self,
        time: Period,
        frequency: Frequency,
        name: impl Into<Cow<'static, str>>,
    ) -> Self {
        self.component_data.sync_points.push(PartialSyncPoint {
            ty: EventType::Repeating { frequency },
            time,
            name: name.into(),
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

    #[inline]
    pub fn interact<C2: Component, T>(
        &self,
        path: &ComponentPath,
        callback: impl FnOnce(&C2) -> T,
    ) -> Option<T> {
        self.registry.interact(path, Period::ZERO, callback)
    }

    #[inline]
    pub fn interact_mut<C2: Component, T>(
        &self,
        path: &ComponentPath,
        callback: impl FnOnce(&mut C2) -> T,
    ) -> Option<T> {
        self.registry.interact_mut(path, Period::ZERO, callback)
    }

    pub fn typed_component_handle<C2: Component>(
        &self,
        path: &ComponentPath,
    ) -> Option<TypedComponentHandle<C2>> {
        self.registry.typed_handle(path)
    }
}
