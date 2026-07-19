//! FluxEMU Runtime
//!
//! The main runtime for the FluxEMU emulator framework

use std::{
    cell::{RefCell, UnsafeCell},
    collections::{HashMap, HashSet},
    fmt::Debug,
    io::Read,
    marker::PhantomData,
    ops::Deref,
    rc::{Rc, Weak},
    sync::Arc,
    time::Duration,
};

use fluxemu_input::{InputId, InputState};
use fluxemu_program::{ProgramManager, ProgramSpecification};
use num::FromPrimitive;
use rustc_hash::FxBuildHasher;
use tracing::Level;

use crate::{
    ComponentPath, RuntimeHandle,
    component::{ComponentRegistryData, LocalComponentRegistryData},
    input::LogicalInputDevice,
    machine::builder::MachineBuilder,
    memory::{AddressSpaceData, AddressSpaceId, LocalMemoryRegistryData, MemoryRegistryData},
    path::ResourcePath,
    persistence::{ErasedCodec, SnapshotMetadata},
    platform::{Platform, TestPlatform},
    scheduler::{Period, Scheduler},
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
    pub(crate) component_registry_data: ComponentRegistryData,
    /// Memory Registry
    pub(crate) memory_registry_data: MemoryRegistryData,
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
        let runtime = CURRENT_THREAD_RUNTIME_HANDLE.with_borrow_mut(|handle| {
            // The weak handle should not be valid by this point
            if handle.upgrade().is_some() {
                panic!("Runtime already entered");
            }

            let runtime = RuntimeHandle::new(self.clone());
            *handle = Rc::downgrade(&runtime);

            runtime
        });

        RuntimeGuard {
            runtime,
            _phantom: PhantomData,
        }
    }

    pub fn program_specification(&self) -> Option<&ProgramSpecification> {
        self.program_specification.as_ref()
    }
}

/// Guard for being inside the context of a runtime
///
/// When this is dropped, the runtime is exited
pub struct RuntimeGuard<'a> {
    runtime: Rc<RuntimeHandle>,
    _phantom: PhantomData<&'a Machine>,
}

impl RuntimeGuard<'_> {
    /// Helper function to advance the scheduler forward by a [Duration]
    ///
    /// Internally converts to the closest representable period
    #[tracing::instrument(skip(self), level = Level::TRACE)]
    pub fn run_duration(&self, allocated_time: Duration) {
        let allocated_time = Period::from_f32(allocated_time.as_secs_f32()).unwrap_or_default();

        self.run(allocated_time);
    }

    /// Drive the scheduler driven components to the current timestamp + the given time
    pub fn run(&self, allocated_time: Period) {
        let mut registry = self.runtime.component_registry();

        self.runtime
            .machine()
            .scheduler
            .run(&mut registry, allocated_time)
    }

    /// Get the last safe time to advance any component to
    pub fn safe_advance_timestamp(&self) -> Period {
        self.runtime.machine().scheduler.safe_advance_timestamp()
    }

    /// Retrieves the timestamp that the machine started with
    pub fn start_time(&self) -> Period {
        self.runtime.machine().scheduler.start_time()
    }

    /// Insert inputs into the machine, storing them into the logical device state and directly giving input devices the
    /// new input change
    #[inline]
    pub fn insert_inputs(
        &self,
        path: &ResourcePath,
        inputs: impl IntoIterator<Item = (InputId, InputState)>,
    ) {
        let logical_input_device = self.runtime.machine().input_devices.get(path).unwrap();

        self.component_registry()
            .interact_dyn(
                path.parent().unwrap(),
                &self.safe_advance_timestamp(),
                |component| {
                    for (input_id, state) in inputs {
                        logical_input_device.set_state(input_id, state);

                        component.handle_input(path.name(), input_id, state);
                    }
                },
            )
            .unwrap();
    }

    pub fn load_snapshot(
        &self,
        _metadata: &SnapshotMetadata,
        mut component_section: impl Read,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let registry = self.component_registry();

        for (path, codec) in &self.runtime.machine().save_codecs {
            registry
                .interact_dyn(path, &Period::default(), |component| {
                    codec.deserialize(component, &mut component_section)
                })
                .unwrap()?;
        }

        Ok(())
    }

    /// List of paths to any audio outputs this machine was created with
    #[inline]
    pub fn audio_outputs(&self) -> &HashSet<ResourcePath> {
        &self.runtime.machine().audio_channels
    }

    /// Input devices this machine was created with
    #[inline]
    pub fn input_devices(&self) -> &HashMap<ResourcePath, Arc<LogicalInputDevice>, FxBuildHasher> {
        &self.runtime.machine().input_devices
    }

    /// Framebuffers this machine was created with
    #[inline]
    pub fn framebuffer_paths(&self) -> &HashSet<ResourcePath> {
        &self.runtime.machine().framebuffers
    }
}

impl<'a> Deref for RuntimeGuard<'a> {
    type Target = RuntimeHandle;

    fn deref(&self) -> &Self::Target {
        &self.runtime
    }
}

impl<'a> Drop for RuntimeGuard<'a> {
    fn drop(&mut self) {
        CURRENT_THREAD_RUNTIME_HANDLE.with_borrow_mut(|runtime| {
            *runtime = Weak::new();
        });

        assert_eq!(
            Rc::strong_count(&self.runtime),
            1,
            "We should be the last owner of the local data"
        );

        // Release all components
        //
        // SAFETY: This is in the drop impl of a !Send !Sync struct, we have exclusive ownership
        unsafe { self.component_registry().release_all() };

        // Release all memory regions
        self.memory_registry().release_all();
    }
}

#[derive(Debug)]
pub(crate) struct ThreadLocalData {
    pub component_registry_data: UnsafeCell<LocalComponentRegistryData>,
    pub memory_registry_data: UnsafeCell<LocalMemoryRegistryData>,
    // Ensure this is !Send and !Sync
    _phantom: PhantomData<*const ()>,
}

impl ThreadLocalData {
    pub fn new(machine: &Machine) -> Self {
        Self {
            component_registry_data: UnsafeCell::new(LocalComponentRegistryData::new(
                &machine.component_registry_data,
            )),
            memory_registry_data: UnsafeCell::new(LocalMemoryRegistryData::new(
                &machine.memory_registry_data,
            )),
            _phantom: PhantomData,
        }
    }
}

thread_local! {
    pub(crate) static CURRENT_THREAD_RUNTIME_HANDLE: RefCell<Weak<RuntimeHandle>> = const { RefCell::new(Weak::new()) };
}
