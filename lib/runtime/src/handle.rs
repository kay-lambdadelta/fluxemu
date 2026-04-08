use std::{
    any::Any,
    cell::RefCell,
    collections::{HashMap, HashSet},
    rc::Rc,
    sync::{Arc, Mutex},
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
    graphics::GraphicsApi,
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
            machine,
            local_component_store: Rc::new(RefCell::new(LocalComponentStore::default())),
        }
    }

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
    pub fn current() -> Self {
        RUNTIME_CONTEXT.with_borrow(|runtime_context| {
            let runtime_api = runtime_context.as_ref().expect("Not inside runtime");

            Self {
                machine: runtime_api.machine.clone(),
                local_component_store: runtime_api.local_component_store.clone(),
            }
        })
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
    /// so handles should be as long lived as possible, by component or frontend structure
    pub fn address_space(&self, address_space_id: AddressSpaceId) -> Option<AddressSpace<'_>> {
        self.machine
            .address_spaces
            .get(&address_space_id)
            .map(|address_space_data| AddressSpace::new(self, address_space_data))
    }

    /// Schedule an event by the [Component]s event type
    ///
    /// This event will fire at the specified timestamp, or if the timestamp is too early (ie: the period for it had already been allocated)
    /// directly after the timestamp
    pub fn schedule_event<C: Component>(
        &self,
        target_path: &ComponentPath,
        requeue_mode: EventMode,
        time: Period,
        data: C::Event,
    ) {
        self.machine.scheduler.event_manager.schedule(
            time,
            target_path.clone(),
            requeue_mode,
            Box::new(data),
        );

        self.machine.scheduler.preemption_signal().event_occurred();
    }

    /// Obtain a handle to the registry
    pub fn registry(&self) -> ComponentRegistry<'_> {
        ComponentRegistry::new(self, &self.machine.registry)
    }

    /// Commit a framebuffer, giving access to the new frame to the frontend
    pub fn commit_framebuffer<G: GraphicsApi>(
        &self,
        path: &ResourcePath,
        callback: impl FnOnce(&mut G::Framebuffer),
    ) {
        let mut framebuffer_guard = self
            .machine
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
    pub fn now(&self) -> Period {
        self.machine.scheduler.now()
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
    ///
    /// HACK: You should use interact to synchronize with the parent component of these framebuffers before accessing them.
    /// In the future, a more unified solution will be found
    pub fn framebuffers(&self) -> &HashMap<ResourcePath, Mutex<Box<dyn Any + Send + Sync>>> {
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
            .interact_dyn_mut(path.parent().unwrap(), self.now(), |component| {
                for (input_id, state) in inputs {
                    logical_input_device.set_state(input_id, state);

                    component.handle_input(path.name(), input_id, state);
                }
            })
            .unwrap();
    }
}
