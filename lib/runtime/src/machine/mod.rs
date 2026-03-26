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

use fluxemu_input::{InputId, InputState};
use fluxemu_program::{ProgramManager, ProgramSpecification};
use num::FromPrimitive;
use rustc_hash::FxBuildHasher;

use crate::{
    RuntimeCurrentThreadContext,
    component::{Component, ComponentHandle, EventType, TypedComponentHandle},
    input::LogicalInputDevice,
    machine::{builder::MachineBuilder, registry::ComponentRegistry},
    memory::{AddressSpace, AddressSpaceId},
    path::{ComponentPath, ResourcePath},
    persistence::{SaveManager, SnapshotManager},
    platform::{Platform, TestPlatform},
    scheduler::{EventRequeueMode, Period, PreemptionSignal, Scheduler},
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
    pub(crate) framebuffers: HashMap<ResourcePath, Mutex<Box<dyn Any + Send + Sync>>>,
    /// All audio outputs this machine has
    pub(crate) audio_outputs: HashSet<ResourcePath>,
    /// The program that this machine was set up with, if any
    program_specification: Option<ProgramSpecification>,
    #[allow(unused)]
    save_manager: SaveManager,
    #[allow(unused)]
    snapshot_manager: SnapshotManager,
    pub(crate) preemption_signals: Vec<Arc<PreemptionSignal>>,
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

    pub fn run_duration(self: &Arc<Self>, allocated_time: Duration) {
        let allocated_time = Period::from_f32(allocated_time.as_secs_f32()).unwrap_or_default();

        self.run(allocated_time);
    }

    pub fn run(self: &Arc<Self>, allocated_time: Period) {
        RuntimeCurrentThreadContext::enter(self.clone(), || self.scheduler.run(allocated_time))
    }

    pub fn now(&self) -> Period {
        self.scheduler.now()
    }

    pub fn start_time(&self) -> Period {
        self.scheduler.start_time()
    }

    pub fn interact<C: Component, T>(
        self: &Arc<Self>,
        path: &ComponentPath,
        callback: impl FnOnce(&C) -> T,
    ) -> Option<T> {
        let now = self.now();

        RuntimeCurrentThreadContext::enter(self.clone(), || {
            self.registry.interact(path, now, callback)
        })
    }

    pub fn interact_mut<C: Component, T: 'static>(
        self: &Arc<Self>,
        path: &ComponentPath,
        callback: impl FnOnce(&mut C) -> T,
    ) -> Option<T> {
        let now = self.now();

        RuntimeCurrentThreadContext::enter(self.clone(), || {
            self.registry.interact_mut(path, now, callback)
        })
    }

    pub fn interact_dyn<T>(
        self: &Arc<Self>,
        path: &ComponentPath,
        callback: impl FnOnce(&dyn Component) -> T,
    ) -> Option<T> {
        let now = self.now();

        RuntimeCurrentThreadContext::enter(self.clone(), || {
            self.registry.interact_dyn(path, now, callback)
        })
    }

    pub fn interact_dyn_mut<T>(
        self: &Arc<Self>,
        path: &ComponentPath,
        callback: impl FnOnce(&mut dyn Component) -> T,
    ) -> Option<T> {
        let now = self.now();

        RuntimeCurrentThreadContext::enter(self.clone(), || {
            self.registry.interact_dyn_mut(path, now, callback)
        })
    }

    pub fn component_handle(&self, path: &ComponentPath) -> Option<ComponentHandle> {
        self.registry.handle(path)
    }

    pub fn typed_component_handle<C: Component>(
        &self,
        path: &ComponentPath,
    ) -> Option<TypedComponentHandle<C>> {
        self.registry.typed_handle(path)
    }

    pub fn audio_outputs(&self) -> &HashSet<ResourcePath> {
        &self.audio_outputs
    }

    pub fn insert_inputs(
        self: &Arc<Self>,
        path: &ResourcePath,
        inputs: impl IntoIterator<Item = (InputId, InputState)>,
    ) {
        let logical_input_device = self.input_devices.get(path).unwrap();

        for (input_id, state) in inputs {
            logical_input_device.set_state(input_id, state);
            let component = self.component_handle(path.parent().unwrap()).unwrap();

            self.scheduler.event_manager.queue(
                Cow::Owned(path.name().to_string()),
                self.now(),
                component,
                EventRequeueMode::Once,
                EventType::input(input_id, state),
            );
        }
    }

    pub fn input_devices(&self) -> &HashMap<ResourcePath, Arc<LogicalInputDevice>, FxBuildHasher> {
        &self.input_devices
    }

    pub fn framebuffers(&self) -> &HashMap<ResourcePath, Mutex<Box<dyn Any + Send + Sync>>> {
        &self.framebuffers
    }

    pub fn program_specification(&self) -> Option<&ProgramSpecification> {
        self.program_specification.as_ref()
    }
}
