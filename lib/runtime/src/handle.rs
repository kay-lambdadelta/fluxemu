use std::{
    any::Any,
    borrow::Cow,
    collections::{HashMap, HashSet},
    marker::PhantomData,
    sync::{Arc, Mutex},
    time::Duration,
};

use fluxemu_input::{InputId, InputState};
use fluxemu_program::ProgramSpecification;
use num::FromPrimitive;
use rustc_hash::FxBuildHasher;

use crate::{
    ComponentPath, ResourcePath,
    component::ComponentRegistry,
    event::{EventRequeueMode, EventType},
    graphics::GraphicsApi,
    input::LogicalInputDevice,
    machine::{Machine, RUNTIME_CONTEXT},
    memory::{AddressSpace, AddressSpaceId, MemoryRemappingCommand},
    scheduler::Period,
};

#[derive(Debug)]
pub struct RuntimeApi {
    machine: Arc<Machine>,
    _phantom: PhantomData<*const ()>,
}

impl RuntimeApi {
    pub(crate) fn new(machine: Arc<Machine>) -> Self {
        RuntimeApi {
            machine,
            _phantom: PhantomData,
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
        RUNTIME_CONTEXT.with(|runtime_context| {
            let runtime_context_guard = runtime_context.borrow();

            let machine = runtime_context_guard
                .as_ref()
                .expect("Not inside runtime")
                .current_machine
                .clone();

            RuntimeApi::new(machine)
        })
    }

    pub(crate) fn machine(&self) -> &Machine {
        &self.machine
    }

    pub fn address_space(&self, address_space_id: AddressSpaceId) -> Option<&AddressSpace> {
        self.machine.address_spaces.get(&address_space_id)
    }

    pub fn insert_event(
        &self,
        target_path: &ComponentPath,
        name: impl Into<Cow<'static, str>>,
        time: Period,
        requeue_mode: EventRequeueMode,
        data: EventType,
    ) {
        let component = self.machine.registry.handle(target_path).unwrap();

        self.machine.scheduler.event_manager.queue(
            name.into(),
            time,
            component,
            requeue_mode,
            data,
        );

        self.machine.scheduler.preemption_signal().event_occurred();
    }

    pub fn remap_address_space(
        &self,
        address_space_id: AddressSpaceId,
        commands: impl IntoIterator<Item = MemoryRemappingCommand>,
    ) {
        let address_space = &self
            .address_space(address_space_id)
            .expect("Unknown address space");

        address_space.remap(commands, &self.machine.registry);
    }

    pub fn registry(&self) -> &ComponentRegistry {
        &self.machine.registry
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
        self.machine.scheduler.run(allocated_time)
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
            let component = self.registry().handle(path.parent().unwrap()).unwrap();

            self.machine.scheduler.event_manager.queue(
                Cow::Owned(path.name().to_string()),
                self.now(),
                component,
                EventRequeueMode::Once,
                EventType::input(input_id, state),
            );
        }
    }
}
