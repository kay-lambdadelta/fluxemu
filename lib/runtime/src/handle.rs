use std::{
    any::Any,
    borrow::Cow,
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
    component::{ComponentRegistry, LocalComponentStore},
    event::{EventRequeueMode, EventType},
    graphics::GraphicsApi,
    input::LogicalInputDevice,
    machine::{Machine, RUNTIME_CONTEXT},
    memory::{AddressSpace, AddressSpaceId},
    scheduler::Period,
};

#[derive(Debug, Clone)]
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
    /// Panics if called outside of an active [`RuntimeGuard`] scope.
    ///
    /// This is intended for use inside component implementations only.
    /// Frontend code should interact with the runtime exclusively through [`RuntimeGuard`], obtained via [`Machine::enter_runtime`].
    #[inline]
    pub fn current() -> Self {
        RUNTIME_CONTEXT.with_borrow(|runtime_context| {
            runtime_context
                .as_ref()
                .expect("Not inside runtime")
                .clone()
        })
    }

    pub(crate) fn machine(&self) -> &Machine {
        &self.machine
    }

    pub(crate) fn local_component_store(&self) -> &RefCell<LocalComponentStore> {
        &self.local_component_store
    }

    pub fn address_space(&self, address_space_id: AddressSpaceId) -> Option<AddressSpace<'_>> {
        self.machine
            .address_spaces
            .get(&address_space_id)
            .map(|address_space_data| AddressSpace::new(self, address_space_data))
    }

    pub fn insert_event(
        &self,
        target_path: &ComponentPath,
        name: impl Into<Cow<'static, str>>,
        time: Period,
        requeue_mode: EventRequeueMode,
        data: EventType,
    ) {
        self.machine.scheduler.event_manager.queue(
            name.into(),
            time,
            target_path.clone(),
            requeue_mode,
            data,
        );

        self.machine.scheduler.preemption_signal().event_occurred();
    }

    pub fn registry(&self) -> ComponentRegistry<'_> {
        ComponentRegistry::new(self, &self.machine.registry)
    }

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

    pub fn program_specification(&self) -> Option<&ProgramSpecification> {
        self.machine.program_specification.as_ref()
    }

    pub fn run_duration(&self, allocated_time: Duration) {
        let allocated_time = Period::from_f32(allocated_time.as_secs_f32()).unwrap_or_default();

        self.run(allocated_time);
    }

    pub fn run(&self, allocated_time: Period) {
        self.machine.scheduler.run(self.registry(), allocated_time)
    }

    pub fn now(&self) -> Period {
        self.machine.scheduler.now()
    }

    pub fn start_time(&self) -> Period {
        self.machine.scheduler.start_time()
    }

    pub fn audio_outputs(&self) -> &HashSet<ResourcePath> {
        &self.machine.audio_outputs
    }

    pub fn input_devices(&self) -> &HashMap<ResourcePath, Arc<LogicalInputDevice>, FxBuildHasher> {
        &self.machine.input_devices
    }

    pub fn framebuffers(&self) -> &HashMap<ResourcePath, Mutex<Box<dyn Any + Send + Sync>>> {
        &self.machine.framebuffers
    }

    pub fn insert_inputs(
        &self,
        path: &ResourcePath,
        inputs: impl IntoIterator<Item = (InputId, InputState)>,
    ) {
        let logical_input_device = self.machine.input_devices.get(path).unwrap();

        for (input_id, state) in inputs {
            logical_input_device.set_state(input_id, state);

            self.machine.scheduler.event_manager.queue(
                Cow::Owned(path.name().to_string()),
                self.now(),
                path.parent().unwrap().clone(),
                EventRequeueMode::Once,
                EventType::input(input_id, state),
            );
        }
    }
}
