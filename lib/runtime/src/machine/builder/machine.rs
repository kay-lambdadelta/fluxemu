use std::{
    borrow::Cow,
    collections::{HashMap, HashSet},
    marker::PhantomData,
    ops::RangeInclusive,
    path::PathBuf,
    sync::{Arc, Mutex},
};

use bytes::Bytes;
use fluxemu_program::{MachineId, ProgramManager, ProgramSpecification, RomId};

use crate::{
    component::ComponentConfig,
    machine::{
        Machine,
        builder::{
            ComponentBuilder, ComponentData, MachineBuilderCommand, PartialSyncPoint,
            RomRequirement, SchedulerParticipation, SealedMachineBuilder,
        },
        graphics::GraphicsRequirements,
        registry::ComponentRegistry,
    },
    memory::{
        Address, AddressSpace, AddressSpaceId, MapTarget, MemoryRemappingCommand, Permissions,
    },
    path::{ComponentPath, ResourcePath},
    persistence::{SaveManager, SnapshotManager},
    platform::Platform,
    scheduler::Scheduler,
};

#[derive(Debug, thiserror::Error)]
pub enum MachineError {
    #[error("Could not find essential ROM")]
    CouldNotFindEssentialRom,
    #[error("{0}")]
    ProgramManager(#[from] fluxemu_program::Error),
}

/// Builder to produce a machine, definition crates will want to use this
pub struct MachineBuilder<'a, P: Platform>
where
    Self: Send,
{
    /// Rom manager
    program_manager: Arc<ProgramManager>,
    /// Save manager
    save_manager: SaveManager,
    /// Snapshot manager
    snapshot_manager: SnapshotManager,
    /// Command queue
    command_queue: Vec<MachineBuilderCommand<'a, P>>,
    /// Program we were opened with
    program_specification: Option<ProgramSpecification>,
    // Next address space
    next_address_space_id: AddressSpaceId,
}

impl<'a, P: Platform> MachineBuilder<'a, P> {
    pub(crate) fn new(
        program_specification: Option<ProgramSpecification>,
        program_manager: Arc<ProgramManager>,
        save_path: Option<PathBuf>,
        snapshot_path: Option<PathBuf>,
    ) -> Self {
        let save_manager = SaveManager::new(save_path);
        let snapshot_manager = SnapshotManager::new(snapshot_path);

        MachineBuilder::<P> {
            save_manager,
            snapshot_manager,
            program_manager,
            program_specification,
            next_address_space_id: AddressSpaceId(0),
            command_queue: Vec::new(),
        }
    }

    pub fn machine_id(&self) -> Option<MachineId> {
        self.program_specification
            .as_ref()
            .map(|program_specification| program_specification.id.machine)
    }

    pub fn program_specification(&self) -> Option<&ProgramSpecification> {
        self.program_specification.as_ref()
    }

    pub fn program_manager(&self) -> &ProgramManager {
        &self.program_manager
    }

    pub fn open_rom(
        &self,
        id: RomId,
        requirement: RomRequirement,
    ) -> Result<Option<Bytes>, fluxemu_program::Error> {
        match self.program_manager.load(id)? {
            Some(bytes) => Ok(Some(bytes)),
            None => match requirement {
                RomRequirement::Sometimes => {
                    tracing::warn!(
                        "Missing ROM {}, machine will be emulated without it but accuracy and \
                         stability may suffer",
                        id
                    );

                    Ok(None)
                }
                RomRequirement::Optional => {
                    tracing::info!(
                        "Missing optional ROM {}, machine will be emulated without it",
                        id
                    );

                    Ok(None)
                }
                RomRequirement::Required => {
                    tracing::error!(
                        "Missing critical ROM {}, emulation cannot occur without it",
                        id
                    );

                    Ok(None)
                }
            },
        }
    }

    #[inline]
    #[must_use]
    pub(super) fn insert_component_with_path<B: ComponentConfig<P> + 'a>(
        path: ComponentPath,
        config: B,
    ) -> MachineBuilderCommand<'a, P> {
        MachineBuilderCommand::CreateComponent {
            path: path.clone(),
            constructor: Box::new(|machine_builder, registry| {
                let mut component_data = ComponentData::new::<B>();

                let component_builder = ComponentBuilder::<P, B::Component> {
                    machine_builder,
                    component_data: &mut component_data,
                    path: &path,
                    registry,
                    _phantom: PhantomData,
                };

                let component = config
                    .build_component(component_builder)
                    .expect("Failed to build component");

                registry.insert_component(
                    path,
                    component_data.scheduler_participation,
                    component_data.preemption_signal.clone(),
                    component_data.save_version,
                    component_data.snapshot_version,
                    component,
                );

                component_data
            }),
        }
    }

    /// Insert a component into the machine
    #[inline]
    pub fn component<B: ComponentConfig<P> + 'a>(
        mut self,
        name: impl Into<Cow<'static, str>>,
        config: B,
    ) -> (Self, ComponentPath) {
        let path = ComponentPath::new(name).unwrap();
        let command = Self::insert_component_with_path(path.clone(), config);

        self.command_queue.push(command);

        (self, path)
    }

    /// Insert a component with a default config
    #[inline]
    pub fn default_component<B: ComponentConfig<P> + Default + 'a>(
        self,
        name: impl Into<Cow<'static, str>>,
    ) -> (Self, ComponentPath) {
        let config = B::default();
        self.component(name, config)
    }

    /// Insert the required information to construct a address space
    pub fn address_space(mut self, width: u8) -> (Self, AddressSpaceId) {
        assert!(
            (width as u32 <= usize::BITS),
            "This host machine cannot handle an address space of {width} bits"
        );

        let address_space_id = self.next_address_space_id;
        self.next_address_space_id.0 = self
            .next_address_space_id
            .0
            .checked_add(1)
            .expect("Too many address spaces");

        self.command_queue
            .push(MachineBuilderCommand::CreateAddressSpace {
                id: address_space_id,
                width,
            });

        (self, address_space_id)
    }

    pub fn memory_map_mirror_read(
        mut self,
        address_space: AddressSpaceId,
        source: RangeInclusive<Address>,
        destination: RangeInclusive<Address>,
    ) -> Self {
        self.command_queue.push(MachineBuilderCommand::MemoryMap {
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

    pub fn memory_map_mirror_write(
        mut self,
        address_space: AddressSpaceId,
        source: RangeInclusive<Address>,
        destination: RangeInclusive<Address>,
    ) -> Self {
        self.command_queue.push(MachineBuilderCommand::MemoryMap {
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

    pub fn memory_map_mirror(
        mut self,
        address_space: AddressSpaceId,
        source: RangeInclusive<Address>,
        destination: RangeInclusive<Address>,
    ) -> Self {
        self.command_queue.push(MachineBuilderCommand::MemoryMap {
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
        mut self,
        address_space: AddressSpaceId,
        name: impl Into<Cow<'static, str>>,
        buffer: Bytes,
    ) -> (Self, ResourcePath) {
        let resource_path = ResourcePath::new(None, name).unwrap();

        self.command_queue.push(MachineBuilderCommand::MemoryMap {
            address_space,
            command: MemoryRemappingCommand::Register {
                path: resource_path.clone(),
                buffer,
            },
        });

        (self, resource_path)
    }

    pub fn memory_map_buffer_read(
        mut self,
        address_space: AddressSpaceId,
        range: RangeInclusive<Address>,
        path: &ResourcePath,
    ) -> Self {
        self.command_queue.push(MachineBuilderCommand::MemoryMap {
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

    pub fn memory_unmap(
        mut self,
        address_space: AddressSpaceId,
        range: RangeInclusive<Address>,
    ) -> Self {
        self.command_queue.push(MachineBuilderCommand::MemoryMap {
            address_space,
            command: MemoryRemappingCommand::Unmap {
                range,
                permissions: Permissions::all(),
            },
        });

        self
    }

    pub fn memory_unmap_read(
        mut self,
        address_space: AddressSpaceId,
        range: RangeInclusive<Address>,
    ) -> Self {
        self.command_queue.push(MachineBuilderCommand::MemoryMap {
            address_space,
            command: MemoryRemappingCommand::Unmap {
                range,
                permissions: Permissions {
                    read: true,
                    write: false,
                },
            },
        });

        self
    }

    pub fn memory_unmap_write(
        mut self,
        address_space: AddressSpaceId,
        range: RangeInclusive<Address>,
    ) -> Self {
        self.command_queue.push(MachineBuilderCommand::MemoryMap {
            address_space,
            command: MemoryRemappingCommand::Unmap {
                range,
                permissions: Permissions {
                    read: false,
                    write: true,
                },
            },
        });

        self
    }

    /// Seal the machine
    pub fn seal(mut self) -> Result<SealedMachineBuilder<P>, MachineError> {
        let mut scheduler = Scheduler::new();
        let mut input_devices = HashMap::default();
        let mut audio_outputs = HashSet::default();
        let mut framebuffers = HashMap::default();
        let mut component_late_initializers = HashMap::default();
        let mut preemption_signals = Vec::default();
        let mut address_spaces = HashMap::default();
        let mut remapping_commands: HashMap<_, Vec<_>> = HashMap::default();
        let mut registry = ComponentRegistry::default();
        let mut graphics_requirements = GraphicsRequirements::default();

        // The machine builder local command queue does not recieve any more items from now
        //
        // All components push to their local queues which get added to the FRONT of the global queue
        //
        // Hence this being logically sound
        let mut global_command_queue = std::mem::take(&mut self.command_queue);

        // Reverse so that we are now popping in the right direction
        global_command_queue.reverse();

        while let Some(command) = global_command_queue.pop() {
            match command {
                MachineBuilderCommand::CreateComponent { path, constructor } => {
                    let mut data = constructor(&mut self, &mut registry);
                    let component_handle = registry.handle(&path).unwrap();

                    component_late_initializers.insert(path.clone(), data.late_initializer);
                    audio_outputs.extend(data.audio_outputs);

                    if data.scheduler_participation == SchedulerParticipation::OnAccess
                        || data.scheduler_participation == SchedulerParticipation::SchedulerDriven
                    {
                        preemption_signals.push(data.preemption_signal);
                    }

                    if data.scheduler_participation == SchedulerParticipation::SchedulerDriven {
                        scheduler.register_driven_component(path, component_handle.clone());
                    }

                    for PartialSyncPoint { ty, time, name } in data.sync_points {
                        scheduler.sync_point_manager.queue(
                            component_handle.clone(),
                            time,
                            ty,
                            name,
                        );
                    }

                    // Append local commands to the start of the global queue
                    data.local_commands.reverse();
                    global_command_queue.extend(data.local_commands);
                }
                MachineBuilderCommand::MemoryMap {
                    address_space,
                    command,
                } => {
                    assert!(address_spaces.contains_key(&address_space), "{:?}", command);

                    remapping_commands
                        .entry(address_space)
                        .or_default()
                        .push(command);
                }
                MachineBuilderCommand::CreateAddressSpace { id, width } => {
                    let address_space = AddressSpace::new(id, width);

                    address_spaces.insert(id, address_space);
                }
                MachineBuilderCommand::CreateFramebuffer { path } => {
                    // Insert dummy type that will be replaced later
                    framebuffers.insert(path, Mutex::new(Box::new(()) as Box<_>));
                }
                MachineBuilderCommand::CreateInputDevice(device) => {
                    input_devices.insert(device.metadata().path.clone(), device);
                }
                MachineBuilderCommand::AddGraphicsRequirements { requirements } => {
                    graphics_requirements = graphics_requirements.clone() | requirements;
                }
            }
        }

        for (id, remapping_commands) in remapping_commands {
            let address_space = address_spaces.get(&id).unwrap();

            address_space.remap(remapping_commands, &registry);
        }

        let machine = Machine {
            scheduler,
            address_spaces,
            input_devices,
            registry,
            framebuffers,
            save_manager: self.save_manager,
            snapshot_manager: self.snapshot_manager,
            program_specification: self.program_specification,
            audio_outputs,
            preemption_signals,
        };

        Ok(SealedMachineBuilder {
            machine: Arc::new(machine),
            component_late_initializers,
            graphics_requirements,
        })
    }
}
