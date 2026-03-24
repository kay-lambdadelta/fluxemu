//! FluxEMU Runtime
//!
//! The main runtime for the FluxEMU emulator framework

use std::{
    any::Any,
    borrow::Cow,
    collections::{HashMap, HashSet},
    fmt::Debug,
    path::PathBuf,
    sync::{Arc, Mutex},
    time::Duration,
};

use fluxemu_program::{ProgramManager, ProgramSpecification};
use num::FromPrimitive;
use rustc_hash::FxBuildHasher;
use serde::{Serialize, de::DeserializeOwned};

use crate::{
    component::{Component, ComponentHandle, TypedComponentHandle},
    graphics::GraphicsApi,
    input::LogicalInputDevice,
    machine::{builder::MachineBuilder, registry::ComponentRegistry},
    memory::{AddressSpace, AddressSpaceId, MemoryRemappingCommand},
    path::{ComponentPath, ResourcePath},
    persistence::{SaveManager, SnapshotManager},
    platform::{Platform, TestPlatform},
    scheduler::{EventType, Frequency, Period, PreemptionSignal, Scheduler},
};

/// Machine builder
pub mod builder;
/// Graphics utilities
pub mod graphics;
pub mod registry;

/// A assembled machine, usable for a further runtime to assist emulation
#[derive(Debug)]
pub struct Machine
where
    Self: Send + Sync,
{
    pub(crate) scheduler: Scheduler,
    /// Memory translation table
    address_spaces: HashMap<AddressSpaceId, AddressSpace, FxBuildHasher>,
    /// All virtual gamepads inserted by components
    input_devices: HashMap<ResourcePath, Arc<LogicalInputDevice>, FxBuildHasher>,
    /// Component Registry
    pub(crate) registry: ComponentRegistry,
    /// All framebuffers this machine has
    framebuffers: HashMap<ResourcePath, Mutex<Box<dyn Any + Send + Sync>>>,
    /// All audio outputs this machine has
    audio_outputs: HashSet<ResourcePath>,
    /// The program that this machine was set up with, if any
    program_specification: Option<ProgramSpecification>,
    #[allow(unused)]
    save_manager: SaveManager,
    #[allow(unused)]
    snapshot_manager: SnapshotManager,
    preemption_signals: Vec<Arc<PreemptionSignal>>,
}

impl Machine {
    pub fn build<'a, P: Platform>(
        program_specification: Option<ProgramSpecification>,
        program_manager: Arc<ProgramManager>,
        save_path: Option<PathBuf>,
        snapshot_path: Option<PathBuf>,
    ) -> MachineBuilder<'a, P> {
        MachineBuilder::<P>::new(
            program_specification,
            program_manager,
            save_path,
            snapshot_path,
        )
    }

    pub fn build_test<'a>(
        program_specification: Option<ProgramSpecification>,
        program_manager: Arc<ProgramManager>,
        save_path: Option<PathBuf>,
        snapshot_path: Option<PathBuf>,
    ) -> MachineBuilder<'a, TestPlatform> {
        Self::build(
            program_specification,
            program_manager,
            save_path,
            snapshot_path,
        )
    }

    pub fn build_test_minimal<'a>() -> MachineBuilder<'a, TestPlatform> {
        Self::build(None, ProgramManager::dummy().unwrap(), None, None)
    }

    #[inline]
    pub fn address_space(&self, address_space_id: AddressSpaceId) -> Option<&AddressSpace> {
        self.address_spaces.get(&address_space_id)
    }

    pub fn remap_address_space(
        &self,
        address_space_id: AddressSpaceId,
        commands: impl IntoIterator<Item = MemoryRemappingCommand>,
    ) {
        let address_space = &self.address_spaces[&address_space_id];
        address_space.remap(commands, &self.registry);
    }

    pub fn insert_sync_point(
        &self,
        time: Period,
        target_path: &ComponentPath,
        name: impl Into<Cow<'static, str>>,
    ) {
        let component = self.registry.handle(target_path).unwrap();

        self.scheduler
            .sync_point_manager
            .queue(component, time, EventType::Once, name.into());

        self.interrupt_in_flight_synchronization();
    }

    pub fn insert_sync_point_with_frequency(
        &self,
        time: Period,
        frequency: Frequency,
        target_path: &ComponentPath,
        name: impl Into<Cow<'static, str>>,
    ) {
        let component = self.registry.handle(target_path).unwrap();

        self.scheduler.sync_point_manager.queue(
            component,
            time,
            EventType::Repeating { frequency },
            name.into(),
        );

        self.interrupt_in_flight_synchronization();
    }

    fn interrupt_in_flight_synchronization(&self) {
        for signal in &self.preemption_signals {
            signal.event_occured();
        }
    }

    pub fn run_duration(&self, allocated_time: Duration) {
        let allocated_time = Period::from_f32(allocated_time.as_secs_f32()).unwrap_or_default();
        self.scheduler.run(allocated_time);
    }

    pub fn run(&self, allocated_time: Period) {
        self.scheduler.run(allocated_time);
    }

    pub fn now(&self) -> Period {
        self.scheduler.now()
    }

    pub fn start_time(&self) -> Period {
        self.scheduler.start_time()
    }

    pub fn interact<C: Component, T>(
        &self,
        path: &ComponentPath,
        callback: impl FnOnce(&C) -> T,
    ) -> Option<T> {
        let now = self.now();

        self.registry.interact(path, now, callback)
    }

    pub fn interact_mut<C: Component, T: 'static>(
        &self,
        path: &ComponentPath,
        callback: impl FnOnce(&mut C) -> T,
    ) -> Option<T> {
        let now = self.now();

        self.registry.interact_mut(path, now, callback)
    }

    pub fn interact_dyn<T>(
        &self,
        path: &ComponentPath,
        callback: impl FnOnce(&dyn Component) -> T,
    ) -> Option<T> {
        let now = self.now();

        self.registry.interact_dyn(path, now, callback)
    }

    pub fn interact_dyn_mut<T>(
        &self,
        path: &ComponentPath,
        callback: impl FnOnce(&mut dyn Component) -> T,
    ) -> Option<T> {
        let now = self.now();

        self.registry.interact_dyn_mut(path, now, callback)
    }

    pub fn typed_handle<C: Component>(
        &self,
        path: &ComponentPath,
    ) -> Option<TypedComponentHandle<C>> {
        self.registry.typed_handle(path)
    }

    pub fn component_handle(&self, path: &ComponentPath) -> Option<ComponentHandle> {
        self.registry.handle(path)
    }

    pub fn audio_outputs(&self) -> &HashSet<ResourcePath> {
        &self.audio_outputs
    }

    pub fn input_devices(&self) -> &HashMap<ResourcePath, Arc<LogicalInputDevice>, FxBuildHasher> {
        &self.input_devices
    }

    pub fn commit_framebuffer<G: GraphicsApi>(
        &self,
        path: &ResourcePath,
        callback: impl FnOnce(&mut G::Texture),
    ) {
        let mut framebuffer_guard = self
            .framebuffers
            .get(path)
            .expect("Could not find framebuffer")
            .lock()
            .unwrap();

        callback(
            framebuffer_guard
                .downcast_mut()
                .expect("This item is not a valid framebuffer"),
        )
    }

    pub fn framebuffers(&self) -> &HashMap<ResourcePath, Mutex<Box<dyn Any + Send + Sync>>> {
        &self.framebuffers
    }

    pub fn program_specification(&self) -> Option<&ProgramSpecification> {
        self.program_specification.as_ref()
    }
}

pub trait Quirks: Serialize + DeserializeOwned + Debug + Clone + Default + 'static {}
impl<T: Serialize + DeserializeOwned + Debug + Clone + Default + 'static> Quirks for T {}
