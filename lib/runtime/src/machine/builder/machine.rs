use std::{
    borrow::Cow,
    collections::{HashMap, HashSet},
    marker::PhantomData,
    ops::RangeInclusive,
    sync::{Arc, Mutex},
};

use bytes::Bytes;
use fluxemu_program::{Manifest, ProgramManager, RomId, SystemId};
use rustc_hash::FxBuildHasher;

use crate::{
    ResourcePath,
    component::{ComponentRegistryData, config::ComponentConfig},
    graphics::GraphicsRequirements,
    input::LogicalInputDevice,
    machine::{
        Machine,
        builder::{
            ComponentBuilder, ComponentData, FramebufferLateInitializer, RomRequirement,
            SealedMachineBuilder,
        },
    },
    memory::{
        Address, AddressSpaceData, AddressSpaceId, MemoryMapCommand, MemoryRegistryData,
        RegionInitializationData,
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
    pub commands: Vec<MemoryMapCommand>,
}

/// Builder to produce a machine, definition crates will want to use this
pub struct MachineBuilder<P: Platform> {
    pub(super) program_manager: Arc<ProgramManager>,
    pub(super) program_specification: Option<Manifest>,
    pub(super) next_address_space_id: AddressSpaceId,
    pub(super) component_registry_data: ComponentRegistryData,
    pub(super) address_spaces: HashMap<AddressSpaceId, AddressSpaceSetupData, FxBuildHasher>,
    pub(super) component_data: HashMap<ComponentPath, ComponentData<P>, FxBuildHasher>,
    pub(super) input_devices: HashMap<ResourcePath, Arc<LogicalInputDevice>, FxBuildHasher>,
    pub(super) framebuffer_late_initializers:
        HashMap<ResourcePath, FramebufferLateInitializer<P>, FxBuildHasher>,
    pub(super) audio_channels: HashSet<ResourcePath, FxBuildHasher>,
    pub(super) required_memory_regions:
        HashMap<ResourcePath, RegionInitializationData, FxBuildHasher>,
    pub(super) scheduler: Scheduler,
}

impl<P: Platform> MachineBuilder<P> {
    pub(crate) fn new(
        program_specification: Option<Manifest>,
        program_manager: Arc<ProgramManager>,
    ) -> Self {
        MachineBuilder::<P> {
            program_manager,
            program_specification,
            next_address_space_id: AddressSpaceId(0),
            component_registry_data: ComponentRegistryData::default(),
            required_memory_regions: HashMap::default(),
            address_spaces: HashMap::default(),
            component_data: HashMap::default(),
            input_devices: HashMap::default(),
            framebuffer_late_initializers: HashMap::default(),
            audio_channels: HashSet::default(),
            scheduler: Scheduler::default(),
        }
    }

    pub fn system_id(&self) -> Option<SystemId> {
        self.program_specification
            .as_ref()
            .map(|program_specification| program_specification.id.system)
    }

    pub fn program_specification(&self) -> Option<&Manifest> {
        self.program_specification.as_ref()
    }

    pub fn program_manager(&self) -> &ProgramManager {
        &self.program_manager
    }

    pub fn open_rom(
        &self,
        id: RomId,
        requirement: RomRequirement,
    ) -> Result<Option<Bytes>, MachineError> {
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

                    Err(MachineError::CouldNotFindEssentialRom)
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

        self.component_registry_data.insert_component(
            path.clone(),
            component,
            component_data.tasks.drain(),
        );

        self.component_data.insert(path, component_data);
    }

    /// Insert a component into the machine
    #[inline]
    pub fn component<B: ComponentConfig<P>>(
        mut self,
        name: impl Into<Cow<'static, str>>,
        config: B,
    ) -> (Self, ComponentPath) {
        let path = ComponentPath::new(name).unwrap();
        self.insert_component_with_path(path.clone(), config);

        (self, path)
    }

    /// Insert a component with a default config
    #[inline]
    pub fn default_component<B: ComponentConfig<P> + Default>(
        self,
        name: impl Into<Cow<'static, str>>,
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

    /// Creates a mutable memory region
    pub fn memory(
        mut self,
        name: impl Into<Cow<'static, str>>,
        size: usize,
        initial_contents: impl IntoIterator<Item = (RangeInclusive<Address>, Bytes)>,
    ) -> (Self, ResourcePath) {
        let path = ResourcePath::new(None, name).unwrap();
        self.insert_memory_region(path.clone(), size, false, initial_contents);

        (self, path)
    }

    /// Creates a mutable memory region that will be committed to saves
    pub fn save_memory(
        mut self,
        name: impl Into<Cow<'static, str>>,
        size: usize,
        initial_contents: impl IntoIterator<Item = (RangeInclusive<Address>, Bytes)>,
    ) -> (Self, ResourcePath) {
        let path = ResourcePath::new(None, name).unwrap();
        self.insert_memory_region(path.clone(), size, true, initial_contents);

        (self, path)
    }

    pub fn map_memory(
        mut self,
        address_space: AddressSpaceId,
        commands: impl IntoIterator<Item = MemoryMapCommand>,
    ) -> Self {
        self.insert_memory_map_commands(address_space, commands);

        self
    }

    pub(super) fn insert_memory_region(
        &mut self,
        path: ResourcePath,
        size: usize,
        sram: bool,
        initial_contents: impl IntoIterator<Item = (RangeInclusive<Address>, Bytes)>,
    ) {
        let previous = self.required_memory_regions.insert(
            path.clone(),
            RegionInitializationData {
                size,
                sram,
                initial_contents: initial_contents.into_iter().collect(),
            },
        );

        assert!(previous.is_none(), "Duplicate memory region path: {path}");
    }

    pub(super) fn insert_memory_map_commands(
        &mut self,
        address_space: AddressSpaceId,
        commands: impl IntoIterator<Item = MemoryMapCommand>,
    ) {
        self.address_spaces
            .get_mut(&address_space)
            .expect("Address space does not exist")
            .commands
            .extend(commands);
    }

    /// Seal the machine
    pub fn seal(self) -> SealedMachineBuilder<P> {
        let mut component_late_initializers = HashMap::default();
        let mut graphics_requirements = GraphicsRequirements::default();
        let mut remapping_commands = Vec::new();
        let mut address_spaces = HashMap::default();

        for (path, component_data) in self.component_data {
            component_late_initializers.insert(path.clone(), component_data.late_initializer);

            graphics_requirements = component_data.graphics_requirements | graphics_requirements;
        }

        for (id, AddressSpaceSetupData { data, commands }) in self.address_spaces {
            address_spaces.insert(id, data);
            remapping_commands.push((id, commands));
        }

        let required_memory_regions = self.required_memory_regions;

        let framebuffers = self
            .framebuffer_late_initializers
            .keys()
            .map(|path| {
                // Put dummy value in hashmap until initialization is complete
                (path.clone(), Mutex::new(Box::new(()) as Box<_>))
            })
            .collect();

        let machine = Arc::new(Machine {
            scheduler: self.scheduler,
            address_spaces,
            input_devices: self.input_devices,
            framebuffers,
            program_specification: self.program_specification,
            audio_channels: self.audio_channels,
            component_registry_data: self.component_registry_data,
            memory_registry_data: MemoryRegistryData::new(required_memory_regions),
        });

        // Initialize address spaces
        let runtime_guard = machine.enter_runtime();
        for (id, commands) in remapping_commands {
            let mut address_space = runtime_guard.address_space(id).unwrap();

            address_space.remap(Period::ZERO, commands);
        }
        drop(runtime_guard);

        SealedMachineBuilder {
            machine,
            component_late_initializers,
            framebuffer_late_initializers: self.framebuffer_late_initializers,
            graphics_requirements,
        }
    }
}
