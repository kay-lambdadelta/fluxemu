use std::{rc::Rc, sync::Arc};

use crate::{
    ComponentPath,
    component::{Component, ComponentRegistry},
    event::EventMode,
    machine::{CURRENT_THREAD_RUNTIME_HANDLE, Machine, ThreadLocalData},
    memory::{AddressSpace, AddressSpaceId},
    scheduler::Period,
};

#[derive(Debug)]
pub struct RuntimeHandle {
    machine: Arc<Machine>,
    local_data: ThreadLocalData,
}

impl RuntimeHandle {
    #[inline]
    pub fn with_current<T>(callback: impl FnOnce(&RuntimeHandle) -> T) -> T {
        CURRENT_THREAD_RUNTIME_HANDLE.with_borrow(|handle| {
            let handle = handle
                .upgrade()
                .expect("This was not called inside an active runtime");

            callback(&handle)
        })
    }

    pub(crate) fn new(machine: Arc<Machine>) -> Rc<RuntimeHandle> {
        Rc::new(RuntimeHandle {
            local_data: ThreadLocalData::new(&machine),
            machine,
        })
    }

    #[inline]
    pub(crate) fn machine(&self) -> &Machine {
        &self.machine
    }

    #[inline]
    pub(crate) fn local_data(&self) -> &ThreadLocalData {
        &self.local_data
    }

    /// Obtain a handle to a address space denoted by the given id
    ///
    /// Note that the handle to the address space contains a cache to make successive loads of the mapping faster
    /// so handles should be as long lived as possible
    #[inline]
    pub fn address_space(&self, address_space_id: AddressSpaceId) -> Option<AddressSpace<'_>> {
        self.machine.address_spaces.get(&address_space_id).map(
            #[inline]
            |address_space_data| AddressSpace::new(self, address_space_data),
        )
    }

    /// Obtain a handle to the component registry
    #[inline]
    pub fn component_registry(&self) -> ComponentRegistry<'_> {
        ComponentRegistry::new(self)
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
    }

    /// Get the current timestamp of your component
    ///
    /// This will NOT be reliable within a synchronization call, and will give the time when the synchronization call started!
    pub fn current_timestamp(&self, path: &ComponentPath) -> Period {
        self.component_registry().get_timestamp(path).unwrap()
    }
}
