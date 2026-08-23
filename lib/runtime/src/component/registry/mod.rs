use std::{
    any::Any,
    borrow::Cow,
    cell::UnsafeCell,
    collections::HashMap,
    fmt::Debug,
    sync::{Arc, Condvar, Mutex},
};

use rustc_hash::FxBuildHasher;

use crate::{
    RuntimeHandle,
    component::{
        Component, ComponentId,
        registry::{
            handle::{ComponentHandle, MutableData},
            timestamp_guard::TimestampGuard,
        },
    },
    path::ComponentPath,
    scheduler::{Context, Period, task::Mode},
};

mod handle;
pub(crate) mod timestamp_guard;

pub(crate) use handle::TaskEntry;

#[derive(Debug)]
struct GlobalComponentMetadata {
    id: ComponentId,
}

#[derive(Debug, Default)]
struct GlobalState {
    global_component_store: HashMap<ComponentId, ComponentHandle, FxBuildHasher>,
    component_condvars: HashMap<ComponentId, Arc<Condvar>, FxBuildHasher>,
}

#[derive(Debug, Default)]
/// The store for components
pub(crate) struct ComponentRegistryData {
    state: Mutex<GlobalState>,
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
        component: C,
        systems: impl IntoIterator<Item = (Cow<'static, str>, TaskEntry)>,
    ) {
        let mut sync_state_guard = self.state.lock().unwrap();

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
                    data: Some(MutableData {
                        component: Box::new(component),
                        systems: systems.into_iter().collect(),
                    }),
                    path: path.clone(),
                },
            )
            .is_some()
        {
            panic!("Component with the same path already exists")
        }

        self.metadata.insert(path, GlobalComponentMetadata { id });
    }
}

/// A registry to interact with components participating in the machine it borrows from
///
/// It has ID and Path based lookup, and cross thread concurrency with automatic synchronization
#[derive(Debug, Clone, Copy)]
pub struct ComponentRegistry<'a> {
    runtime: &'a RuntimeHandle,
}

impl<'a> ComponentRegistry<'a> {
    #[inline]
    pub(crate) fn new(runtime: &'a RuntimeHandle) -> Self {
        Self { runtime }
    }

    #[inline]
    fn data(&self) -> &ComponentRegistryData {
        &self.runtime.machine().component_registry_data
    }

    #[inline]
    fn local_data(&self) -> &UnsafeCell<LocalComponentRegistryData> {
        &self.runtime.local_data().component_registry_data
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
        self.synchronize_component(id, target_timestamp);

        let mut data = {
            let store = unsafe { &mut *self.runtime.local_data().component_registry_data.get() };

            store
                .get_slot(id)
                .as_mut()
                .unwrap()
                .data
                .take()
                .expect("Component is reentrant on itself")
        };

        // Record the current timestamp on the thread local
        let _guard = TimestampGuard::enter(*target_timestamp);

        let item = callback(data.component.as_mut());

        let store = unsafe { &mut *self.runtime.local_data().component_registry_data.get() };
        unsafe { store.get_slot(id).as_mut().unwrap_unchecked() }.data = Some(data);

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
        &self,
        id: impl Into<ComponentIdentifier<'b>>,
    ) -> Option<ComponentId> {
        match id.into() {
            ComponentIdentifier::Id(id) => {
                if (id.0 as usize) < self.data().required_local_store_size() {
                    Some(id)
                } else {
                    None
                }
            }
            ComponentIdentifier::Path(path) => Some(self.data().metadata.get(path)?.id),
        }
    }

    fn synchronize_component(&self, id: ComponentId, target_timestamp: &Period) {
        loop {
            let (mut data, path) = {
                let store = unsafe { &mut *self.local_data().get() };
                let handle = self.fetch_or_acquire_component(id, store);

                (handle.data.take().unwrap(), handle.path.clone())
            };

            if data.systems.is_empty() {
                let store =
                    unsafe { &mut *self.runtime.local_data().component_registry_data.get() };

                store.get_slot(id).as_mut().unwrap().data = Some(data);

                return;
            }

            let mut earliest_hazard = None;
            let mut any_incomplete = false;

            for (name, entry) in data.systems.iter_mut() {
                while !entry.started || entry.current_timestamp < *target_timestamp {
                    entry.started = true;

                    let previous_timestamp = entry.current_timestamp;
                    let mut last_attempted_allocation = Period::ZERO;

                    let mut context = Context {
                        runtime: self.runtime,
                        current_timestamp: &mut entry.current_timestamp,
                        target_timestamp: *target_timestamp,
                        last_attempted_allocation: &mut last_attempted_allocation,
                    };

                    let next_deadline = entry.task.run(data.component.as_mut(), &mut context);

                    if last_attempted_allocation == Period::ZERO {
                        let path = path.clone().into_resource(name.clone()).unwrap();

                        panic!("System for {path} did not attempt to allocate time");
                    }

                    if next_deadline == entry.current_timestamp {
                        let path = path.clone().into_resource(name.clone()).unwrap();

                        panic!(
                            "Delta between requested next deadline by system is zero for {path}"
                        );
                    }

                    if entry.mode == Mode::Always {
                        let path = path.clone().into_resource(name.clone()).unwrap();

                        self.runtime
                            .machine()
                            .scheduler
                            .queue
                            .reschedule_task(path.clone(), next_deadline);
                    }

                    if entry.current_timestamp == previous_timestamp {
                        let queue = &self.runtime.machine().scheduler.queue;

                        if let Some(blocking_deadline) = queue.next_deadline()
                            && blocking_deadline < *target_timestamp
                        {
                            earliest_hazard = Some(
                                earliest_hazard.map_or(blocking_deadline, |hazard: Period| {
                                    hazard.min(blocking_deadline)
                                }),
                            );
                            any_incomplete = true;
                        }

                        break;
                    }
                }
            }

            let store = unsafe { &mut *self.runtime.local_data().component_registry_data.get() };
            store.get_slot(id).as_mut().unwrap().data = Some(data);

            if !any_incomplete {
                return;
            }

            self.runtime
                .machine()
                .scheduler
                .queue
                .handle_deadlines_before(earliest_hazard.unwrap(), self);
        }
    }

    #[inline]
    fn fetch_or_acquire_component<'b>(
        &self,
        id: ComponentId,
        local_data: &'b mut LocalComponentRegistryData,
    ) -> &'b mut ComponentHandle {
        if local_data.get_slot(id).is_some() {
            let component_handle = local_data.get_slot(id);

            component_handle.as_mut().unwrap()
        } else {
            self.acquire_component_from_global_store(id, local_data)
        }
    }

    #[cold]
    fn acquire_component_from_global_store<'b>(
        &self,
        id: ComponentId,
        local_data: &'b mut LocalComponentRegistryData,
    ) -> &'b mut ComponentHandle {
        let mut sync_state_guard = self.data().state.lock().unwrap();

        loop {
            let Some(handle) = sync_state_guard.global_component_store.remove(&id) else {
                // Give components back so others can potentially access them
                self.release_all_inner(&mut sync_state_guard, local_data);

                // Get condvar
                let condvar = sync_state_guard
                    .component_condvars
                    .entry(id)
                    .or_default()
                    .clone();

                // Wait for someone to give up that component
                sync_state_guard = condvar.wait(sync_state_guard).unwrap();

                // Try again until we can acquire that component
                continue;
            };

            let slot = local_data.get_slot(id);
            *slot = Some(handle);

            return slot.as_mut().unwrap();
        }
    }

    pub(crate) unsafe fn release_all(&self) {
        let mut sync_state_guard = self.data().state.lock().unwrap();
        let local_data = unsafe { &mut *self.runtime.local_data().component_registry_data.get() };

        self.release_all_inner(&mut sync_state_guard, local_data);
    }

    /// Release all components currently available for releasing
    fn release_all_inner(
        &self,
        sync_state: &mut GlobalState,
        local_data: &mut LocalComponentRegistryData,
    ) {
        for (id, slot) in local_data.iter_mut() {
            // Check if the slot is occupied and the handle isn't borrowed
            if let Some(handle) = slot
                && handle.data.is_some()
            {
                let handle = slot.take().unwrap();

                if sync_state
                    .global_component_store
                    .insert(id, handle)
                    .is_some()
                {
                    panic!("Component shadowed by another component");
                }

                let Some(condvar) = sync_state.component_condvars.get(&id) else {
                    continue;
                };

                // Notify one lucky thread!
                condvar.notify_one();
            }
        }
    }

    pub(crate) fn id_for_path(&self, path: &ComponentPath) -> Option<ComponentId> {
        Some(self.data().metadata.get(path)?.id)
    }
}

/// An identifier for a component, either by its ID or path.
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
pub(crate) struct LocalComponentRegistryData(Vec<Option<ComponentHandle>>);

impl LocalComponentRegistryData {
    pub fn new(registry_data: &ComponentRegistryData) -> Self {
        LocalComponentRegistryData(Vec::from_iter(
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
