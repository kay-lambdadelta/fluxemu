use std::{cell::UnsafeCell, ops::Deref, rc::Rc, sync::Arc};

use fluxemu_program::ProgramSpecification;

use crate::{
    ComponentPath,
    component::{Component, ComponentRegistry, LocalComponentStore},
    event::EventMode,
    machine::{Machine, RUNTIME_CONTEXT},
    memory::{AddressSpace, AddressSpaceId},
    scheduler::Period,
};

#[derive(Debug, Clone)]
pub struct RuntimeApi<M: Deref<Target = Machine>> {
    machine: M,
    local_component_store: Rc<UnsafeCell<LocalComponentStore>>,
}

impl RuntimeApi<Arc<Machine>> {
    #[inline]
    pub fn current() -> Self {
        RUNTIME_CONTEXT.with_borrow(|context| context.as_ref().unwrap().clone())
    }
}

impl<M: Deref<Target = Machine>> RuntimeApi<M> {
    pub(crate) fn new(machine: M) -> Self {
        RuntimeApi {
            local_component_store: Rc::new(UnsafeCell::new(LocalComponentStore::new(
                &machine.registry_data,
            ))),
            machine,
        }
    }

    pub(crate) fn new_with_local_store(
        machine: M,
        local_component_store: Rc<UnsafeCell<LocalComponentStore>>,
    ) -> Self {
        RuntimeApi {
            local_component_store,
            machine,
        }
    }

    #[inline]
    pub fn as_ref(&self) -> RuntimeApi<&Machine> {
        RuntimeApi {
            local_component_store: self.local_component_store.clone(),
            machine: &*self.machine,
        }
    }

    #[inline]
    pub(crate) fn machine(&self) -> &Machine {
        &self.machine
    }

    #[inline]
    pub(crate) fn local_component_store(&self) -> &Rc<UnsafeCell<LocalComponentStore>> {
        &self.local_component_store
    }

    /// Obtain a handle to a address space denoted by the given id
    ///
    /// Note that the handle to the address space contains a cache to make successive loads of the mapping faster
    /// so handles should be as long lived as possible
    #[inline]
    pub fn address_space(&self, address_space_id: AddressSpaceId) -> Option<AddressSpace<'_>> {
        self.machine.address_spaces.get(&address_space_id).map(
            #[inline]
            |address_space_data| AddressSpace::new(self.registry(), address_space_data),
        )
    }

    /// Obtain a handle to the registry
    #[inline]
    pub fn registry(&self) -> ComponentRegistry<'_> {
        ComponentRegistry::new(self.as_ref(), &self.machine.registry_data)
    }

    /// Gain access to the program specification the [Machine] was created with
    #[inline]
    pub fn program_specification(&self) -> Option<&ProgramSpecification> {
        self.machine.program_specification.as_ref()
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
        self.machine.scheduler.event_manager.schedule(
            time,
            target_path.clone(),
            requeue_mode,
            Box::new(data),
        );

        self.machine.scheduler.preemption_signal().event_occurred();
    }

    /// Get the current timestamp of your component
    ///
    /// This will NOT be reliable within a synchronization call, and will give the time when the synchronization call started!
    pub fn current_timestamp(&self, path: &ComponentPath) -> Period {
        self.registry().get_timestamp(path).unwrap()
    }
}
