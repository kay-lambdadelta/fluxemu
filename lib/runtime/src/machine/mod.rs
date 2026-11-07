use std::{
    any::Any,
    collections::{HashMap, HashSet},
    fmt::Debug,
    hash::BuildHasher,
    rc::{Rc, Weak},
    sync::{Arc, Mutex, OnceLock, RwLock, RwLockReadGuard, RwLockWriteGuard},
    time::Duration,
};

use fluxemu_graphics::api::GraphicsApi;
use fluxemu_input::{InputId, InputState};
use fluxemu_program::{Manifest, ProgramManager};
use num::FromPrimitive;
use rustc_hash::FxBuildHasher;
use tracing::Level;

use crate::{
    CURRENT_THREAD_RUNTIME_HANDLE, DispatchGuard, RuntimeHandle,
    component::{ComponentRegistry, ComponentRegistryData},
    graphics::DisplayGeometry,
    input::LogicalInputDevice,
    machine::builder::MachineBuilder,
    memory::{AddressSpace, AddressSpaceData, AddressSpaceId, MemoryRegistryData},
    path::ResourcePath,
    platform::{Platform, TestPlatform},
    scheduler::{Period, Scheduler},
};

/// Builder pattern constructor for a [`Machine`]
pub mod builder;

pub trait FramebufferListener: Debug + Send + Sync {
    fn framebuffer_written(&self);
}

#[derive(Debug)]
pub(crate) struct FramebufferSlot {
    pub(crate) framebuffer: Box<dyn Any + Send + Sync>,
    pub(crate) display_geometry: DisplayGeometry,
}

/// The main context of the runtime, encapsulating all state and resources for a running machine.
#[derive(Debug)]
pub struct Machine
where
    Self: Send + Sync,
{
    pub(crate) scheduler: Scheduler,
    pub(crate) runtime_gate: RwLock<()>,
    /// Memory translation table
    pub(crate) address_spaces: HashMap<AddressSpaceId, AddressSpaceData, FxBuildHasher>,
    /// All virtual gamepads inserted by components
    pub(crate) input_devices: HashMap<ResourcePath, Arc<LogicalInputDevice>, FxBuildHasher>,
    /// Component Registry
    pub(crate) component_registry_data: ComponentRegistryData,
    /// Memory Registry
    pub(crate) memory_registry_data: MemoryRegistryData,
    /// All framebuffers this machine has
    pub(crate) framebuffers: HashMap<ResourcePath, Mutex<FramebufferSlot>, FxBuildHasher>,
    pub(crate) framebuffer_listener: OnceLock<Box<dyn FramebufferListener>>,
    /// All audio outputs this machine has
    pub(crate) audio_channels: HashSet<ResourcePath, FxBuildHasher>,
    /// The program that this machine was set up with, if any
    pub(crate) program_specification: Option<Manifest>,
}

impl Machine {
    /// Creates a new [`MachineBuilder`] for the given platform and specifications
    pub fn build<P: Platform>(
        program_specification: Option<Manifest>,
        program_manager: Arc<ProgramManager>,
    ) -> MachineBuilder<P> {
        MachineBuilder::<P>::new(program_specification, program_manager)
    }

    /// Creates a new [`MachineBuilder`] for the test platform, which is to be used with unit tests only
    pub fn build_test(
        program_specification: Option<Manifest>,
        program_manager: Arc<ProgramManager>,
    ) -> MachineBuilder<TestPlatform> {
        Self::build(program_specification, program_manager)
    }

    /// Creates a new [`MachineBuilder`] for the test platform with dummy defaults
    ///
    /// This will probably be completely unsuccessful at running any real world program.
    /// It should only be used for runtime/component sanity unit tests
    pub fn build_test_minimal() -> MachineBuilder<TestPlatform> {
        Self::build(None, ProgramManager::new([]).unwrap())
    }

    /// Enter the runtime for this machine on this thread
    ///
    /// # Panics
    ///
    /// Panics if the runtime is already entered on this thread
    #[must_use]
    pub fn enter_runtime(self: &Arc<Self>) -> RuntimeGuard<'_> {
        Self::assert_not_entered();

        let gate = RuntimeGate::Shared(self.runtime_gate.read().unwrap());

        self.install_runtime(gate)
    }

    #[must_use]
    pub fn enter_runtime_exclusive(self: &Arc<Self>) -> ExclusiveRuntimeGuard<'_> {
        Self::assert_not_entered();

        let gate = RuntimeGate::Exclusive(self.runtime_gate.write().unwrap());

        ExclusiveRuntimeGuard(self.install_runtime(gate))
    }

    fn assert_not_entered() {
        CURRENT_THREAD_RUNTIME_HANDLE.with_borrow(|handle| {
            // The weak handle should not be valid by this point
            if handle.upgrade().is_some() {
                panic!("Runtime already entered");
            }
        });
    }

    fn install_runtime<'a>(self: &'a Arc<Self>, gate: RuntimeGate<'a>) -> RuntimeGuard<'a> {
        let runtime = CURRENT_THREAD_RUNTIME_HANDLE.with_borrow_mut(|handle| {
            let runtime = RuntimeHandle::new(self.clone());
            *handle = Rc::downgrade(&runtime);

            runtime
        });

        RuntimeGuard {
            runtime,
            _gate: gate,
        }
    }

    pub fn program_specification(&self) -> Option<&Manifest> {
        self.program_specification.as_ref()
    }

    pub fn set_framebuffer_listener(&self, listener: impl FramebufferListener + 'static) {
        self.framebuffer_listener.set(Box::new(listener)).unwrap();
    }
}

/// Guard for being inside the context of a runtime
///
/// When this is dropped, the runtime is exited
pub struct RuntimeGuard<'a> {
    runtime: Rc<RuntimeHandle>,
    _gate: RuntimeGate<'a>,
}

enum RuntimeGate<'a> {
    Shared(#[allow(unused)] RwLockReadGuard<'a, ()>),
    Exclusive(#[allow(unused)] RwLockWriteGuard<'a, ()>),
}

pub struct ExclusiveRuntimeGuard<'a>(RuntimeGuard<'a>);

impl ExclusiveRuntimeGuard<'_> {
    pub(crate) fn runtime(&self) -> &RuntimeHandle {
        &self.0.runtime
    }
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
        let registry = self.runtime.component_registry();

        self.runtime
            .machine()
            .scheduler
            .run(registry, allocated_time);
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
                self.safe_advance_timestamp(),
                |component| {
                    for (input_id, state) in inputs {
                        logical_input_device.set_state(input_id, state);

                        component.handle_input(path.name(), input_id, state);
                    }
                },
            )
            .unwrap();
    }

    /// List all the framebuffers inside this machine
    pub fn list_framebuffers(&self) -> impl Iterator<Item = &ResourcePath> {
        self.runtime.machine().framebuffers.keys()
    }

    pub fn read_framebuffer<G: GraphicsApi, T>(
        &self,
        path: &ResourcePath,
        target_timestamp: Period,
        callback: impl FnOnce(&G::Framebuffer, &DisplayGeometry) -> T,
    ) -> Option<T> {
        // Ensure we are advanced to at least this timestamp
        self.runtime.component_registry().interact_dyn(
            path.parent().unwrap(),
            target_timestamp,
            |_| {
                let slot = self.runtime.machine().framebuffers.get(path)?;
                let slot = slot.lock().unwrap();

                let framebuffer = slot.framebuffer.downcast_ref()?;

                Some(callback(framebuffer, &slot.display_geometry))
            },
        )?
    }

    /// List of paths to any audio outputs this machine was created with
    #[inline]
    pub fn audio_outputs(&self) -> &HashSet<ResourcePath, impl BuildHasher> {
        &self.runtime.machine().audio_channels
    }

    /// Input devices this machine was created with
    #[inline]
    pub fn input_devices(
        &self,
    ) -> &HashMap<ResourcePath, Arc<LogicalInputDevice>, impl BuildHasher> {
        &self.runtime.machine().input_devices
    }

    #[inline]
    pub fn address_space(&self, address_space_id: AddressSpaceId) -> Option<AddressSpace<'_>> {
        self.runtime.address_space(address_space_id)
    }

    #[inline]
    pub fn component_registry(&self) -> ComponentRegistry<'_> {
        self.runtime.component_registry()
    }
}

impl Drop for RuntimeGuard<'_> {
    fn drop(&mut self) {
        DispatchGuard::clear();
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
        //
        // SAFETY: No memory operation can be in progress outside of the runtime
        unsafe { self.runtime.memory_registry().release_all() };
    }
}
