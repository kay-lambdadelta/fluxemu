//! FluxEMU Runtime
//!
//! The main runtime for the FluxEMU emulator framework

use std::{
    any::Any,
    cell::RefCell,
    collections::{HashMap, HashSet},
    fmt::Debug,
    marker::PhantomData,
    ops::Deref,
    path::PathBuf,
    sync::{Arc, Mutex},
};

use fluxemu_program::{ProgramManager, ProgramSpecification};
use rustc_hash::FxBuildHasher;

use crate::{
    RuntimeApi,
    component::ComponentRegistryData,
    input::LogicalInputDevice,
    machine::builder::MachineBuilder,
    memory::{AddressSpaceData, AddressSpaceId},
    path::ResourcePath,
    persistence::{SaveManager, SnapshotManager},
    platform::{Platform, TestPlatform},
    scheduler::Scheduler,
};

/// Machine builder
pub mod builder;

#[derive(Debug)]
pub struct Machine
where
    Self: Send + Sync,
{
    pub(crate) scheduler: Scheduler,
    /// Memory translation table
    pub(crate) address_spaces: HashMap<AddressSpaceId, AddressSpaceData, FxBuildHasher>,
    /// All virtual gamepads inserted by components
    pub(crate) input_devices: HashMap<ResourcePath, Arc<LogicalInputDevice>, FxBuildHasher>,
    /// Component Registry
    pub(crate) registry: ComponentRegistryData,
    /// All framebuffers this machine has
    pub(crate) framebuffers: HashMap<ResourcePath, Mutex<Box<dyn Any + Send + Sync>>>,
    /// All audio outputs this machine has
    pub(crate) audio_outputs: HashSet<ResourcePath>,
    /// The program that this machine was set up with, if any
    pub(crate) program_specification: Option<ProgramSpecification>,
    #[allow(unused)]
    pub(crate) save_manager: SaveManager,
    #[allow(unused)]
    pub(crate) snapshot_manager: SnapshotManager,
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

    /// Enter the runtime for this machine on this thread
    #[must_use]
    pub fn enter_runtime(self: &Arc<Self>) -> RuntimeGuard<'_> {
        RUNTIME_CONTEXT.with(|runtime_context| {
            let mut runtime_context_guard = runtime_context.borrow_mut();

            if runtime_context_guard.is_some() {
                panic!("Runtime already entered");
            }

            *runtime_context_guard = Some(RuntimeApi::new(self.clone()));
        });

        RuntimeGuard {
            api: RuntimeApi::current(),
            _phantom: PhantomData,
        }
    }
}

/// Guard for being inside the context of a runtime
///
/// When this is dropped, the runtime is exited
pub struct RuntimeGuard<'a> {
    api: RuntimeApi,
    _phantom: PhantomData<&'a ()>,
}

impl<'a> Deref for RuntimeGuard<'a> {
    type Target = RuntimeApi;

    fn deref(&self) -> &Self::Target {
        &self.api
    }
}

impl<'a> Drop for RuntimeGuard<'a> {
    fn drop(&mut self) {
        RUNTIME_CONTEXT.with(|runtime_context| {
            let runtime_context = runtime_context.borrow_mut().take();

            // Clear the local context
            if let Some(context) = runtime_context {
                // Release all components
                self.registry()
                    .unmitigate_components(&mut context.local_component_store().borrow_mut());
            } else {
                unreachable!("Runtime exited without entering");
            }
        });
    }
}

thread_local! {
    pub(crate) static RUNTIME_CONTEXT: RefCell<Option<RuntimeApi>> = const { RefCell::new(None) };
}
