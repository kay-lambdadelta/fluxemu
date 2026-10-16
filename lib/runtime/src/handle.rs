use std::{
    cell::{Cell, RefCell, UnsafeCell},
    marker::PhantomData,
    rc::{Rc, Weak},
    sync::Arc,
};

use fluxemu_graphics::api::GraphicsApi;

use crate::{
    ComponentPath, ResourcePath,
    component::{Component, ComponentId, ComponentRegistry, LocalComponentRegistryData},
    machine::Machine,
    memory::{AddressSpace, AddressSpaceId, LocalMemoryRegistryData},
    scheduler::{Period, event::EventMode},
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

    /// Fire an event by the [Component]'s event type
    ///
    /// This event will fire at the specified timestamp, or if the timestamp is too early (ie: the period for it had already been allocated) directly after the timestamp
    pub fn schedule_event_at<C: Component>(
        &self,
        target_path: &ComponentPath,
        requeue_mode: EventMode,
        at: Period,
        data: C::Event,
    ) {
        self.machine.scheduler.queue.schedule_event(
            target_path.clone(),
            at,
            requeue_mode,
            Box::new(data),
        );
    }

    /// Fire relative to the current timestamp for this interaction call
    ///
    /// See [`Self::schedule_event_at`] for more information
    pub fn schedule_event_relative<C: Component>(
        &self,
        target_path: &ComponentPath,
        mode: EventMode,
        delay: Period,
        data: C::Event,
    ) {
        self.schedule_event_at::<C>(target_path, mode, self.current_timestamp() + delay, data);
    }

    /// Fire this event as soon as possible
    ///
    /// See [`Self::schedule_event_at`] for more information
    pub fn schedule_event_now<C: Component>(
        &self,
        target_path: &ComponentPath,
        mode: EventMode,
        data: C::Event,
    ) {
        self.schedule_event_at::<C>(target_path, mode, self.current_timestamp(), data);
    }

    /// Write to a framebuffer
    pub fn write_framebuffer<G: GraphicsApi, T>(
        &self,
        path: &ResourcePath,
        callback: impl FnOnce(&mut G::Framebuffer) -> T,
    ) -> Option<T> {
        let framebuffer = self.machine.framebuffers.get(path)?;
        let mut framebuffer_guard = framebuffer.lock().unwrap();

        let framebuffer = (*framebuffer_guard).downcast_mut()?;

        Some(callback(framebuffer))
    }

    /// Get the current timestamp of your component
    ///
    /// This will NOT be reliable within a system call, and will give the time when the system call started!
    pub fn current_timestamp(&self) -> Period {
        self.current_dispatch()
            .expect("This function only works within an interact call")
            .timestamp
    }

    pub fn current_component(&self) -> ComponentId {
        self.current_dispatch()
            .expect("This function only works within an interact call")
            .component
    }

    #[inline]
    pub(crate) fn current_dispatch(&self) -> Option<Dispatch> {
        let component = CURRENT_DISPATCH_COMPONENT.get()?;

        Some(Dispatch {
            component,
            timestamp: CURRENT_DISPATCH_TIMESTAMP.get(),
        })
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
    static CURRENT_DISPATCH_COMPONENT: Cell<Option<ComponentId>> = const { Cell::new(None) };
    static CURRENT_DISPATCH_TIMESTAMP: Cell<Period> = const { Cell::new(Period::ZERO) };
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Dispatch {
    pub component: ComponentId,
    pub timestamp: Period,
}

pub(crate) struct DispatchGuard {
    previous_component: Option<ComponentId>,
    previous_timestamp: Period,
}

impl DispatchGuard {
    #[inline]
    pub fn enter(component: ComponentId, timestamp: Period) -> Self {
        Self {
            previous_component: CURRENT_DISPATCH_COMPONENT.replace(Some(component)),
            previous_timestamp: CURRENT_DISPATCH_TIMESTAMP.replace(timestamp),
        }
    }

    #[inline]
    pub fn advance(timestamp: Period) {
        CURRENT_DISPATCH_TIMESTAMP.set(timestamp);
    }

    pub fn clear() {
        CURRENT_DISPATCH_COMPONENT.set(None);
        CURRENT_DISPATCH_TIMESTAMP.set(Period::ZERO);
    }
}

impl Drop for DispatchGuard {
    #[inline]
    fn drop(&mut self) {
        CURRENT_DISPATCH_COMPONENT.set(self.previous_component);
        CURRENT_DISPATCH_TIMESTAMP.set(self.previous_timestamp);
    }
}
