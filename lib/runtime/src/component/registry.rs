use std::{
    any::{Any, TypeId},
    collections::HashMap,
    fmt::Debug,
    ops::DerefMut,
    sync::{Arc, Condvar, Mutex},
};

use rustc_hash::FxBuildHasher;

use crate::{
    RuntimeApi,
    component::{Component, ComponentId},
    machine::Machine,
    path::ComponentPath,
    scheduler::{Period, SynchronizationContext},
};

#[derive(Debug)]
pub(crate) struct ComponentHandle {
    current_timestamp: Period,
    synchronize: bool,
    component: Option<Box<dyn Component>>,
}

#[derive(Debug)]
struct GlobalComponentMetadata {
    id: ComponentId,
    type_id: TypeId,
}

#[derive(Debug, Default)]
struct GlobalSyncState {
    global_component_store: HashMap<ComponentId, ComponentHandle, FxBuildHasher>,
    threads_awaiting_component: HashMap<ComponentId, Arc<Condvar>, FxBuildHasher>,
}

#[derive(Debug, Default)]
/// The store for components
pub(crate) struct ComponentRegistryData {
    sync_state: Mutex<GlobalSyncState>,
    metadata: HashMap<ComponentPath, GlobalComponentMetadata, FxBuildHasher>,
    next_component_id: u16,
}

impl ComponentRegistryData {
    pub(crate) fn required_local_store_size(&self) -> usize {
        self.next_component_id as usize
    }

    pub fn insert_component<C: Component>(
        &mut self,
        path: ComponentPath,
        synchronize: bool,
        component: C,
    ) {
        let mut sync_state_guard = self.sync_state.lock().unwrap();

        let id = ComponentId(self.next_component_id);
        self.next_component_id = self
            .next_component_id
            .checked_add(1)
            .expect("Too many components");

        if sync_state_guard
            .global_component_store
            .insert(
                id,
                ComponentHandle {
                    current_timestamp: Period::default(),
                    synchronize,
                    component: Some(Box::new(component)),
                },
            )
            .is_some()
        {
            panic!("Component with the same path already exists")
        }

        self.metadata.insert(
            path,
            GlobalComponentMetadata {
                id,
                type_id: TypeId::of::<C>(),
            },
        );
    }
}

/// A registry to interact with components participating in the machine it borrows from
///
/// It has ID and Path based lookup, and cross thread concurrency with automatic synchronization
#[derive(Debug, Clone)]
pub struct ComponentRegistry<'a> {
    runtime: RuntimeApi<&'a Machine>,
    data: &'a ComponentRegistryData,
}

impl<'a> ComponentRegistry<'a> {
    #[inline]
    pub(crate) fn new(runtime: RuntimeApi<&'a Machine>, data: &'a ComponentRegistryData) -> Self {
        Self { runtime, data }
    }

    /// The interaction is performed the exact same way as [`interact_dyn`](Self::interact_dyn), except it downcasts the component to `C` before calling the callback.
    ///
    /// Prefer this if you are a component author and you need to directly interact with another component
    #[inline]
    pub fn interact<'b, C: Component, T>(
        &'b self,
        id: impl Into<ComponentIdentifier<'b>>,
        target_timestamp: &Period,
        callback: impl FnOnce(&mut C) -> T,
    ) -> Option<T> {
        self.interact_dyn(
            id,
            target_timestamp,
            #[inline]
            |component| {
                let component = (component as &mut dyn Any).downcast_mut::<C>().unwrap();

                callback(component)
            },
        )
    }

    /// # Safety
    ///
    /// The ID must be valid for the registry instance.
    #[inline]
    pub(crate) unsafe fn interact_dyn_id_unchecked<T>(
        &self,
        id: ComponentId,
        target_timestamp: &Period,
        callback: impl FnOnce(&mut dyn Component) -> T,
    ) -> T {
        let cell = self.runtime.local_component_store();

        // Move the component handle to our thread and check if it needs synchronization
        let needs_sync = {
            // SAFETY: No active borrows
            let store = unsafe { &mut *cell.get() };
            let handle = self.fetch_or_acquire_component(id, store);

            if !handle.synchronize {
                // Working around bad LLVM optimization
                //
                // SAFETY: This is essentially the same as a dereference and assign
                unsafe {
                    std::ptr::copy_nonoverlapping(
                        target_timestamp,
                        &mut handle.current_timestamp,
                        1,
                    );
                }
            }

            handle.synchronize
        };

        // Synchronize if required
        if needs_sync {
            self.synchronize_component(id, target_timestamp);
        }

        // Extract the component
        let mut component = {
            // SAFETY: No active borrows
            let store = unsafe { &mut *cell.get() };
            let handle = store.get_slot(id).as_mut().unwrap();

            handle
                .component
                .take()
                .expect("Component is reentrant on itself")
        };

        // Do the callback and get the return
        let item = callback(component.deref_mut());

        // Clean up
        {
            // SAFETY:
            //  The first operation is sound because there are currently no overlapping borrows
            //  The second is sound because nothing could have been moved out of that slot, due to us owning the component
            //  The third operation is not unsafe, but it is logically sound to forget the `None` in the slot and save some instructions

            let store = unsafe { &mut *cell.get() };
            let handle = unsafe { store.get_slot(id).as_mut().unwrap_unchecked() };
            std::mem::forget(handle.component.replace(component));
        }

        item
    }

    /// Interact with a component by its ID or path via a closure, returning the output of that closure if the component could be found
    ///
    /// If the component has not yet reached the timestamp given, it will be caught up to it before the interaction occurs.
    /// Components are cached in a thread local store, meaning that repeated interactions with the same component are very cheap.
    ///
    /// # Concurrent interaction behavior
    ///
    /// This function will automatically block when another thread has the component in its per thread store, until it is released.
    ///
    /// Additionally, right before this function blocks, it is guaranteed to return the non-borrowed components in the local store to the global store.
    #[inline]
    pub fn interact_dyn<'b, T>(
        &'b self,
        id: impl Into<ComponentIdentifier<'b>>,
        target_timestamp: &Period,
        callback: impl FnOnce(&mut dyn Component) -> T,
    ) -> Option<T> {
        let id = self.convert_identifier(id)?;

        // SAFETY: convert_identifier validates the ID, returning None if it isn't within the local store size
        Some(unsafe { self.interact_dyn_id_unchecked(id, target_timestamp, callback) })
    }

    #[inline]
    fn convert_identifier<'b>(
        &'a self,
        id: impl Into<ComponentIdentifier<'b>>,
    ) -> Option<ComponentId> {
        match id.into() {
            ComponentIdentifier::Id(id) => {
                if (id.0 as usize) < self.data.required_local_store_size() {
                    Some(id)
                } else {
                    None
                }
            }
            ComponentIdentifier::Path(path) => Some(self.data.metadata.get(path)?.id),
        }
    }

    #[cold]
    fn synchronize_component(&self, id: ComponentId, target_timestamp: &Period) {
        let cell = self.runtime.local_component_store();

        let mut current_timestamp;

        let (delta, mut component) = {
            let store = unsafe { &mut *cell.get() };
            let handle = store.get_slot(id).as_mut().unwrap();

            let Some(delta) = target_timestamp.checked_sub(handle.current_timestamp) else {
                return;
            };

            current_timestamp = handle.current_timestamp;

            (delta, handle.component.take().unwrap())
        };

        if delta == Period::ZERO || !component.needs_work(&current_timestamp, &delta) {
            let store = unsafe { &mut *cell.get() };
            store.get_slot(id).as_mut().unwrap().component = Some(component);

            return;
        }

        loop {
            let mut last_attempted_allocation = Period::ZERO;

            let context = SynchronizationContext {
                runtime: self.runtime.clone(),
                current_timestamp: &mut current_timestamp,
                target_timestamp: *target_timestamp,
                last_attempted_allocation: &mut last_attempted_allocation,
            };

            component.synchronize(context);

            let delta = target_timestamp - current_timestamp;
            let needs_work = component.needs_work(&current_timestamp, &delta);

            assert_ne!(
                last_attempted_allocation,
                Period::ZERO,
                "Synchronization attempt for component did not attempt to allocate time"
            );

            let hazard_timestamp = {
                let store = unsafe { &mut *cell.get() };
                let handle = store.get_slot(id).as_mut().unwrap();

                handle.component = Some(component);
                handle.current_timestamp = current_timestamp;

                handle.current_timestamp + last_attempted_allocation
            };

            if !needs_work {
                return;
            }

            self.runtime
                .machine()
                .scheduler
                .event_manager
                .consume(self.clone(), hazard_timestamp);

            {
                let store = unsafe { &mut *cell.get() };
                let handle = self.fetch_or_acquire_component(id, store);

                component = handle.component.take().unwrap();
                current_timestamp = handle.current_timestamp;
            }
        }
    }

    #[inline]
    fn fetch_or_acquire_component<'b>(
        &self,
        id: ComponentId,
        local_component_store: &'b mut LocalComponentStore,
    ) -> &'b mut ComponentHandle {
        if local_component_store.get_slot(id).is_some() {
            let component_handle = local_component_store.get_slot(id);

            component_handle.as_mut().unwrap()
        } else {
            self.acquire_component_from_global_store(id, local_component_store)
        }
    }

    #[cold]
    fn acquire_component_from_global_store<'b>(
        &self,
        id: ComponentId,
        local_component_store: &'b mut LocalComponentStore,
    ) -> &'b mut ComponentHandle {
        let mut sync_state_guard = self.data.sync_state.lock().unwrap();

        loop {
            let Some(handle) = sync_state_guard.global_component_store.remove(&id) else {
                // Give components back so others can potentially access them
                self.release_all_components_inner(local_component_store, &mut sync_state_guard);

                // Get condvar
                let condvar = sync_state_guard
                    .threads_awaiting_component
                    .entry(id)
                    .or_default()
                    .clone();

                // Wait for someone to give up that component
                sync_state_guard = condvar.wait(sync_state_guard).unwrap();

                // Try again until we can acquire that component
                continue;
            };

            let slot = local_component_store.get_slot(id);
            *slot = Some(handle);

            return slot.as_mut().unwrap();
        }
    }

    pub(crate) fn release_all_components(&self, local_component_store: &mut LocalComponentStore) {
        let mut sync_state_guard = self.data.sync_state.lock().unwrap();

        self.release_all_components_inner(local_component_store, &mut sync_state_guard);
    }

    /// Release all components currently available for releasing
    fn release_all_components_inner(
        &self,
        local_component_store: &mut LocalComponentStore,
        sync_state: &mut GlobalSyncState,
    ) {
        for (id, slot) in local_component_store.iter_mut() {
            // Check if the slot is occupied and the handle isn't borrowed
            if let Some(handle) = slot
                && handle.component.is_some()
            {
                let handle = slot.take().unwrap();

                if sync_state
                    .global_component_store
                    .insert(id, handle)
                    .is_some()
                {
                    panic!("Component shadowed by another component");
                }

                let Some(condvar) = sync_state.threads_awaiting_component.get(&id) else {
                    continue;
                };

                // Notify one lucky thread!
                condvar.notify_one();
            }
        }
    }

    pub(crate) fn path_to_id(&self, path: &ComponentPath) -> Option<ComponentId> {
        Some(self.data.metadata.get(path)?.id)
    }

    pub(crate) fn get_timestamp<'b>(
        &'b self,
        id: impl Into<ComponentIdentifier<'b>>,
    ) -> Option<Period> {
        let id = self.convert_identifier(id)?;

        let timestamp = {
            let store = unsafe { &mut *self.runtime.local_component_store().get() };
            self.fetch_or_acquire_component(id, store).current_timestamp
        };

        Some(timestamp)
    }

    pub(crate) fn typeid(&self, path: &ComponentPath) -> Option<TypeId> {
        Some(self.data.metadata.get(path)?.type_id)
    }
}

/// An identifier for a component, either by its ID or path.
///
/// You should not have to use this type directly, instead rely on its `From` impls
pub enum ComponentIdentifier<'a> {
    /// ID
    Id(ComponentId),
    /// Path
    Path(&'a ComponentPath),
}

impl<'a> From<&'a ComponentPath> for ComponentIdentifier<'a> {
    fn from(path: &'a ComponentPath) -> Self {
        ComponentIdentifier::Path(path)
    }
}

impl<'a> From<ComponentId> for ComponentIdentifier<'a> {
    fn from(id: ComponentId) -> Self {
        ComponentIdentifier::Id(id)
    }
}

#[derive(Debug)]
pub(crate) struct LocalComponentStore(Vec<Option<ComponentHandle>>);

impl LocalComponentStore {
    pub fn new(registry_data: &ComponentRegistryData) -> Self {
        LocalComponentStore(Vec::from_iter(
            std::iter::repeat_with(|| None).take(registry_data.required_local_store_size()),
        ))
    }

    #[inline]
    fn get_slot(&mut self, id: ComponentId) -> &mut Option<ComponentHandle> {
        debug_assert!(id.0 < self.0.len() as u16);

        // SAFETY:
        //  All ids are validated against required_local_store_size in convert_identifier before this function
        //  Component store has a static size
        unsafe { self.0.get_unchecked_mut(id.0 as usize) }
    }

    #[inline]
    fn iter_mut(&mut self) -> impl Iterator<Item = (ComponentId, &mut Option<ComponentHandle>)> {
        self.0
            .iter_mut()
            .enumerate()
            .map(|(id, component_handle)| (ComponentId(id as u16), component_handle))
    }
}
