use std::{
    borrow::Cow,
    cell::RefCell,
    collections::{HashMap, HashSet},
    rc::Rc,
    sync::Arc,
    time::Duration,
};

use fluxemu_input::{InputId, InputState};
use fluxemu_program::ProgramSpecification;
use num::FromPrimitive;
use rustc_hash::FxBuildHasher;

use crate::{
    ComponentPath, ResourcePath,
    component::{Component, ComponentRegistry, LocalComponentStore},
    event::EventMode,
    input::LogicalInputDevice,
    machine::{Machine, RUNTIME_CONTEXT},
    memory::{AddressSpace, AddressSpaceId},
    scheduler::Period,
};

#[derive(Debug)]
pub struct RuntimeApi {
    machine: Arc<Machine>,
    local_component_store: Rc<RefCell<LocalComponentStore>>,
}

impl RuntimeApi {
    pub(crate) fn new(machine: Arc<Machine>) -> Self {
        RuntimeApi {
            local_component_store: Rc::new(RefCell::new(LocalComponentStore::new(
                &machine.registry_data,
            ))),
            machine,
        }
    }

    pub(crate) fn duplicate(&self) -> Self {
        Self {
            machine: self.machine.clone(),
            local_component_store: self.local_component_store.clone(),
        }
    }

    pub(crate) fn machine(&self) -> &Machine {
        &self.machine
    }

    pub(crate) fn local_component_store(&self) -> &RefCell<LocalComponentStore> {
        &self.local_component_store
    }

    /// Obtain a handle to a address space denoted by the given id
    ///
    /// Note that the handle to the address space contains a cache to make successive loads of the mapping faster
    /// so handles should be as long lived as possible
    pub fn address_space(&self, address_space_id: AddressSpaceId) -> Option<AddressSpace<'_>> {
        self.machine
            .address_spaces
            .get(&address_space_id)
            .map(|address_space_data| AddressSpace::new(self, address_space_data))
    }

    /// Obtain a handle to the registry
    pub fn registry(&self) -> ComponentRegistry<'_> {
        ComponentRegistry::new(self, &self.machine.registry_data)
    }

    /// Gain access to the program specification the [Machine] was created with
    pub fn program_specification(&self) -> Option<&ProgramSpecification> {
        self.machine.program_specification.as_ref()
    }

    /// Helper function to advance the scheduler forward by a [Duration]
    ///
    /// Internally converts to the closest representable period
    pub fn run_duration(&self, allocated_time: Duration) {
        let allocated_time = Period::from_f32(allocated_time.as_secs_f32()).unwrap_or_default();

        self.run(allocated_time);
    }

    /// Drive the scheduler driven components to the current timestamp + the given time
    pub fn run(&self, allocated_time: Period) {
        self.machine.scheduler.run(self.registry(), allocated_time)
    }

    /// Get the last safe time to advance any component to
    pub fn safe_advance_timestamp(&self) -> Period {
        self.machine.scheduler.safe_advance_timestamp()
    }

    /// Retrieves the timestamp that the machine started with
    pub fn start_time(&self) -> Period {
        self.machine.scheduler.start_time()
    }

    /// List of paths to any audio outputs this machine was created with
    pub fn audio_outputs(&self) -> &HashSet<ResourcePath> {
        &self.machine.audio_outputs
    }

    /// Input devices this machine was created with
    pub fn input_devices(&self) -> &HashMap<ResourcePath, Arc<LogicalInputDevice>, FxBuildHasher> {
        &self.machine.input_devices
    }

    /// Framebuffers this machine was created with
    pub fn framebuffer_paths(&self) -> &HashSet<ResourcePath> {
        &self.machine.framebuffers
    }

    /// Insert inputs into the machine, storing them into the logical device state and directly giving input devices the
    /// new input change
    pub fn insert_inputs(
        &self,
        path: &ResourcePath,
        inputs: impl IntoIterator<Item = (InputId, InputState)>,
    ) {
        let logical_input_device = self.machine.input_devices.get(path).unwrap();

        self.registry()
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
}

#[derive(Debug)]
pub struct ComponentRuntimeApi<'a> {
    runtime: RuntimeApi,
    component: Cow<'a, ComponentPath>,
}

impl<'a> ComponentRuntimeApi<'a> {
    /// Retrieves the current runtime api context for this thread.
    ///
    /// # Panics
    /// Panics if called outside of an active runtime scope.
    ///
    /// This should be impossible as the runtime guards component access behind the runtime scope
    ///
    ///
    /// This is intended for use inside component implementations only.
    ///
    /// Frontend code should interact with the runtime exclusively through the guard obtained via [`Machine::enter_runtime`].
    #[inline]
    pub fn current(component: impl Into<Cow<'a, ComponentPath>>) -> Self {
        RUNTIME_CONTEXT.with_borrow(|runtime_context| {
            let runtime = runtime_context.as_ref().expect("Not inside runtime");
            let runtime = runtime.duplicate();

            Self {
                runtime,
                component: component.into(),
            }
        })
    }

    /// Retrieves the timestamp that the machine started with
    pub fn start_time(&self) -> Period {
        self.runtime.start_time()
    }

    /// Gain access to the program specification the [Machine] was created with
    pub fn program_specification(&self) -> Option<&ProgramSpecification> {
        self.runtime.program_specification()
    }

    /// Obtain a handle to the registry
    pub fn registry(&self) -> ComponentRegistry<'_> {
        self.runtime.registry()
    }

    /// Obtain a handle to a address space denoted by the given id
    ///
    /// Note that the handle to the address space contains a cache to make successive loads of the mapping faster
    /// so handles should be as long lived as possible
    pub fn address_space(&self, address_space_id: AddressSpaceId) -> Option<AddressSpace<'_>> {
        self.runtime.address_space(address_space_id)
    }

    /// Schedule an event by the [Component]s event type
    ///
    /// This event will fire at the specified timestamp, or if the timestamp is too early (ie: the period for it had already been allocated) directly after the timestamp
    pub fn schedule_event<C: Component>(
        &self,
        target_path: &ComponentPath,
        requeue_mode: EventMode,
        time: Period,
        data: C::Event,
    ) {
        self.runtime.machine.scheduler.event_manager.schedule(
            time,
            target_path.clone(),
            requeue_mode,
            Box::new(data),
        );

        self.runtime
            .machine
            .scheduler
            .preemption_signal()
            .event_occurred();
    }

    /// Get the current timestamp of your component
    ///
    /// This will NOT be reliable within a synchronization call, and will give the time when the synchronization call started!
    pub fn current_timestamp(&self) -> Period {
        self.registry()
            .get_timestamp(self.component.as_ref())
            .unwrap()
    }

    pub fn suggest_save(&self) {
        todo!()
    }
}
