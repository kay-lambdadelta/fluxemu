use std::{
    collections::{HashMap, HashSet},
    marker::PhantomData,
    ops::RangeInclusive,
    sync::Arc,
};

use bytes::Bytes;
use fluxemu_program::{MachineId, ProgramManager, ProgramSpecification, RomId};
use rustc_hash::FxBuildHasher;

use crate::{
    ResourcePath,
    component::{ComponentRegistryData, config::ComponentConfig},
    graphics::GraphicsRequirements,
    input::LogicalInputDevice,
    machine::{
        Machine,
        builder::{ComponentBuilder, ComponentData, RomRequirement, SealedMachineBuilder},
    },
    memory::{
        Address, AddressSpaceData, AddressSpaceId, MapTarget, MemoryRemappingCommand, Permissions,
    },
    path::ComponentPath,
    platform::Platform,
    scheduler::{Period, Scheduler},
};

#[derive(Debug, thiserror::Error)]
pub enum MachineError {
    #[error("Could not find essential ROM")]
    CouldNotFindEssentialRom,
    #[error("{0}")]
    ProgramManager(#[from] fluxemu_program::Error),
}

pub(super) struct AddressSpaceSetupData {
    pub data: AddressSpaceData,
    pub commands: Vec<MemoryRemappingCommand>,
}

/// Builder to produce a machine, definition crates will want to use this
pub struct MachineBuilder<P: Platform> {
    pub(super) program_manager: Arc<ProgramManager>,
    pub(super) program_specification: Option<ProgramSpecification>,
    pub(super) next_address_space_id: AddressSpaceId,
    pub(super) registry_data: ComponentRegistryData,
    pub(super) address_spaces: HashMap<AddressSpaceId, AddressSpaceSetupData>,
    pub(super) component_data: HashMap<ComponentPath, ComponentData<P>>,
    pub(super) input_devices: HashMap<ResourcePath, Arc<LogicalInputDevice>, FxBuildHasher>,
    pub(super) framebuffers: HashSet<ResourcePath>,
    pub(super) audio_channels: HashSet<ResourcePath>,
    pub(super) scheduler: Scheduler,
}

impl<P: Platform> MachineBuilder<P> {
    pub(crate) fn new(
        program_specification: Option<ProgramSpecification>,
        program_manager: Arc<ProgramManager>,
    ) -> Self {
        MachineBuilder::<P> {
            program_manager,
            program_specification,
            next_address_space_id: AddressSpaceId(0),
            registry_data: ComponentRegistryData::default(),
            address_spaces: HashMap::default(),
            component_data: HashMap::default(),
            input_devices: HashMap::default(),
            framebuffers: HashSet::default(),
            audio_channels: HashSet::default(),
            scheduler: Scheduler::new(),
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
    pub(super) fn insert_component_with_path<B: ComponentConfig<P>>(
        &mut self,
        path: ComponentPath,
        config: B,
    ) {
        let mut component_data = ComponentData::new::<B>();

        let component_builder = ComponentBuilder::<P, B::Component> {
            machine_builder: self,
            component_data: &mut component_data,
            path: &path,
            _phantom: PhantomData,
        };

        let component = config
            .build_component(component_builder)
            .expect("Failed to build component");

        self.registry_data.insert_component(
            path.clone(),
            component_data.scheduler_participation.is_some(),
            component,
        );

        self.component_data.insert(path, component_data);
    }

    /// Insert a component into the machine
    #[inline]
    pub fn component<B: ComponentConfig<P>>(
        mut self,
        name: &str,
        config: B,
    ) -> (Self, ComponentPath) {
        let path = ComponentPath::new(name.into()).unwrap();
        self.insert_component_with_path(path.clone(), config);

        (self, path)
    }

    /// Insert a component with a default config
    #[inline]
    pub fn default_component<B: ComponentConfig<P> + Default>(
        self,
        name: &str,
    ) -> (Self, ComponentPath) {
        let config = B::default();
        self.component(name, config)
    }

    /// Insert the required information to construct a address space
    pub fn address_space(mut self, width: u8) -> (Self, AddressSpaceId) {
        let address_space_id = self.next_address_space_id;
        self.next_address_space_id.0 = self
            .next_address_space_id
            .0
            .checked_add(1)
            .expect("Too many address spaces");

        self.address_spaces.insert(
            address_space_id,
            AddressSpaceSetupData {
                data: AddressSpaceData::new(address_space_id, width),
                commands: Vec::default(),
            },
        );

        (self, address_space_id)
    }

    pub fn memory_map_mirror_read(
        mut self,
        address_space: AddressSpaceId,
        source: RangeInclusive<Address>,
        destination: RangeInclusive<Address>,
    ) -> Self {
        self.address_spaces
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

    pub fn memory_map_mirror_write(
        mut self,
        address_space: AddressSpaceId,
        source: RangeInclusive<Address>,
        destination: RangeInclusive<Address>,
    ) -> Self {
        self.address_spaces
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

    pub fn memory_map_mirror(
        mut self,
        address_space: AddressSpaceId,
        source: RangeInclusive<Address>,
        destination: RangeInclusive<Address>,
    ) -> Self {
        self.address_spaces
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
        mut self,
        address_space: AddressSpaceId,
        range: RangeInclusive<Address>,
        memory: impl Into<Bytes>,
    ) -> Self {
        self.address_spaces
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

    pub fn memory_unmap(
        mut self,
        address_space: AddressSpaceId,
        range: RangeInclusive<Address>,
    ) -> Self {
        self.address_spaces
            .get_mut(&address_space)
            .unwrap()
            .commands
            .push(MemoryRemappingCommand::Unmap {
                range,
                permissions: Permissions::ALL,
            });

        self
    }

    pub fn memory_unmap_read(
        mut self,
        address_space: AddressSpaceId,
        range: RangeInclusive<Address>,
    ) -> Self {
        self.address_spaces
            .get_mut(&address_space)
            .unwrap()
            .commands
            .push(MemoryRemappingCommand::Unmap {
                range,
                permissions: Permissions::READ,
            });

        self
    }

    pub fn memory_unmap_write(
        mut self,
        address_space: AddressSpaceId,
        range: RangeInclusive<Address>,
    ) -> Self {
        self.address_spaces
            .get_mut(&address_space)
            .unwrap()
            .commands
            .push(MemoryRemappingCommand::Unmap {
                range,
                permissions: Permissions::WRITE,
            });

        self
    }

    /// Seal the machine
    pub fn seal(self) -> SealedMachineBuilder<P> {
        let mut component_late_initializers = HashMap::default();
        let mut graphics_requirements = GraphicsRequirements::default();
        let mut remapping_commands = HashMap::new();
        let mut address_spaces = HashMap::default();
        let mut save_codecs = HashMap::default();
        let mut snapshot_codecs = HashMap::default();

        for (path, component_data) in self.component_data {
            component_late_initializers.insert(path.clone(), component_data.late_initializer);

            graphics_requirements = component_data.graphics_requirements | graphics_requirements;

            if let Some(save_codec) = component_data.save_codec {
                save_codecs.insert(path.clone(), save_codec);
            }

            if let Some(snapshot_codec) = component_data.snapshot_codec {
                snapshot_codecs.insert(path, snapshot_codec);
            }
        }

        for (id, AddressSpaceSetupData { data, commands }) in self.address_spaces {
            address_spaces.insert(id, data);
            remapping_commands.insert(id, commands);
        }

        let machine = Arc::new(Machine {
            scheduler: self.scheduler,
            address_spaces,
            input_devices: self.input_devices,
            registry_data: self.registry_data,
            framebuffers: self.framebuffers,
            program_specification: self.program_specification,
            audio_channels: self.audio_channels,
            save_codecs,
            snapshot_codecs,
        });

        // Initialize address spaces
        let runtime_guard = machine.enter_runtime();
        for (id, commands) in remapping_commands {
            let address_space = runtime_guard.address_space(id).unwrap();
            address_space.remap(Period::default(), commands);
        }
        drop(runtime_guard);

        SealedMachineBuilder {
            machine,
            component_late_initializers,
            graphics_requirements,
        }
    }
}
