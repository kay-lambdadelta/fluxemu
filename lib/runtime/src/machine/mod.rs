//! FluxEMU Runtime
//!
//! The main runtime for the FluxEMU emulator framework

use std::{
    cell::RefCell,
    collections::{HashMap, HashSet},
    fmt::Debug,
    marker::PhantomData,
    ops::Deref,
    sync::Arc,
};

use fluxemu_program::{ProgramManager, ProgramSpecification};
use rustc_hash::FxBuildHasher;

use crate::{
    ComponentPath, RuntimeApi,
    component::ComponentRegistryData,
    input::LogicalInputDevice,
    machine::builder::MachineBuilder,
    memory::{AddressSpaceData, AddressSpaceId},
    path::ResourcePath,
    persistence::ErasedCodec,
    platform::{Platform, TestPlatform},
    scheduler::Scheduler,
};

/// Builder pattern constructor for a [`Machine`]
pub mod builder;

/// The main context of the runtime, encapsulating all state and resources for a running machine.
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
    pub(crate) registry_data: ComponentRegistryData,
    /// All framebuffers this machine has
    pub(crate) framebuffers: HashSet<ResourcePath>,
    /// All audio outputs this machine has
    pub(crate) audio_channels: HashSet<ResourcePath>,
    /// The program that this machine was set up with, if any
    pub(crate) program_specification: Option<ProgramSpecification>,
    pub(crate) save_codecs: HashMap<ComponentPath, Box<dyn ErasedCodec>>,
    pub(crate) snapshot_codecs: HashMap<ComponentPath, Box<dyn ErasedCodec>>,
}

impl Machine {
    /// Creates a new [`MachineBuilder`] for the given platform and specifications
    pub fn build<P: Platform>(
        program_specification: Option<ProgramSpecification>,
        program_manager: Arc<ProgramManager>,
    ) -> MachineBuilder<P> {
        MachineBuilder::<P>::new(program_specification, program_manager)
    }

    /// Creates a new [`MachineBuilder`] for the test platform, which is to be used with unit tests only
    pub fn build_test(
        program_specification: Option<ProgramSpecification>,
        program_manager: Arc<ProgramManager>,
    ) -> MachineBuilder<TestPlatform> {
        Self::build(program_specification, program_manager)
    }

    /// Creates a new [`MachineBuilder`] for the test platform with dummy defaults
    ///
    /// This will probably be completely unsuccessful at running any real world program.
    /// It should only be used for runtime/component sanity unit tests
    pub fn build_test_minimal() -> MachineBuilder<TestPlatform> {
        Self::build(None, ProgramManager::dummy().unwrap())
    }

    /// Enter the runtime for this machine on this thread
    ///
    /// # Panics
    ///
    /// Panics if the runtime is already entered on this thread
    #[must_use]
    pub fn enter_runtime(self: &Arc<Self>) -> RuntimeGuard<'_> {
        let runtime = RUNTIME_CONTEXT.with(|runtime_context| {
            let mut runtime_context_guard = runtime_context.borrow_mut();

            if runtime_context_guard.is_some() {
                panic!("Runtime already entered");
            }

            let runtime = RuntimeApi::new(self.clone());

            *runtime_context_guard = Some(runtime.duplicate());

            runtime
        });

        RuntimeGuard {
            runtime,
            _phantom: PhantomData,
        }
    }
}

/// Guard for being inside the context of a runtime
///
/// When this is dropped, the runtime is exited
pub struct RuntimeGuard<'a> {
    runtime: RuntimeApi,
    // Make sure the lifetime is constrained, and do not allow this guard to cross thread boundaries
    _phantom: PhantomData<(&'a (), *mut ())>,
}

impl<'a> Deref for RuntimeGuard<'a> {
    type Target = RuntimeApi;

    fn deref(&self) -> &Self::Target {
        &self.runtime
    }
}

impl<'a> Drop for RuntimeGuard<'a> {
    fn drop(&mut self) {
        RUNTIME_CONTEXT.with(|runtime_context| {
            let runtime_context = runtime_context.borrow_mut().take();

            // Clear the local context
            if let Some(context) = runtime_context {
                // Release all components
                //
                // SAFETY: This is in the drop impl of a !Send !Sync struct, we have exclusive ownership
                context
                    .registry()
                    .release_all_components(unsafe { &mut *context.local_component_store().get() });
            } else {
                unreachable!("Runtime exited without entering");
            }
        });
    }
}

thread_local! {
    pub(crate) static RUNTIME_CONTEXT: RefCell<Option<RuntimeApi>> = const { RefCell::new(None) };
}
