use std::{
    any::Any,
    collections::HashMap,
    fmt::Debug,
    ops::DerefMut,
    thread::{Thread, ThreadId},
};

use rustc_hash::FxBuildHasher;

use crate::{
    RuntimeApi,
    component::{Component, ComponentId},
    path::ComponentPath,
    persistence::PersistanceFormatVersion,
    scheduler::{Period, SynchronizationContext},
};

#[derive(Debug)]
pub(crate) struct ComponentHandle {
    current_timestamp: Period,
    synchronize: bool,
    save_version: Option<PersistanceFormatVersion>,
    snapshot_version: PersistanceFormatVersion,
    component: Option<Box<dyn Component>>,
}

#[derive(Debug, Default)]
/// The store for components
pub(crate) struct ComponentRegistryData {
    global_component_store: scc::HashMap<ComponentId, ComponentHandle, FxBuildHasher>,
    threads_awaiting_component: scc::HashMap<ComponentId, HashMap<ThreadId, Thread>, FxBuildHasher>,

    path2id: HashMap<ComponentPath, ComponentId, FxBuildHasher>,
    next_component_id: u32,
}

impl ComponentRegistryData {
    pub(crate) fn required_local_store_size(&self) -> usize {
        self.next_component_id as usize
    }

    pub fn insert_component<C: Component>(
        &mut self,
        path: ComponentPath,
        save_version: Option<PersistanceFormatVersion>,
        snapshot_version: PersistanceFormatVersion,
        synchronize: bool,
        component: C,
    ) {
        let id = ComponentId(self.next_component_id);
        self.next_component_id = self
            .next_component_id
            .checked_add(1)
            .expect("Too many components");

        self.global_component_store
            .insert_sync(
                id,
                ComponentHandle {
                    current_timestamp: Period::default(),
                    synchronize,
                    save_version,
                    snapshot_version,
                    component: Some(Box::new(component)),
                },
            )
            .unwrap_or_else(|_| {
                panic!("Component with the same path already exists");
            });

        self.path2id.insert(path, id);
    }
}

#[derive(Debug, Clone, Copy)]
pub struct ComponentRegistry<'a> {
    runtime: &'a RuntimeApi,
    data: &'a ComponentRegistryData,
}

impl<'a> ComponentRegistry<'a> {
    pub(crate) fn new(runtime: &'a RuntimeApi, data: &'a ComponentRegistryData) -> Self {
        Self { runtime, data }
    }

    #[inline]
    pub fn interact<'b, C: Component, T>(
        &'b self,
        id: impl Into<ComponentIdentifier<'b>>,
        time: Period,
        callback: impl FnOnce(&mut C) -> T,
    ) -> Option<T> {
        self.interact_dyn(
            id,
            time,
            #[inline]
            |component| {
                let component = (component as &mut dyn Any).downcast_mut::<C>().unwrap();

                callback(component)
            },
        )
    }

    #[inline]
    pub fn interact_dyn<'b, T>(
        &'b self,
        id: impl Into<ComponentIdentifier<'b>>,
        target_timestamp: Period,
        callback: impl FnOnce(&mut dyn Component) -> T,
    ) -> Option<T> {
        let id = match id.into() {
            ComponentIdentifier::Id(id) => id,
            ComponentIdentifier::Path(path) => self.data.path2id.get(path).copied()?,
        };

        let mut guard = self.runtime.local_component_store().borrow_mut();
        let handle = self.fetch_or_acquire_component(id, &mut guard);

        if !handle.synchronize {
            // Just simply update the timestamp
            handle.current_timestamp = target_timestamp;
        } else {
            // Drop guard for synchronization
            drop(guard);

            // Synchronize the component
            self.synchronize_component(id, target_timestamp);

            // Reacquire guard
            guard = self.runtime.local_component_store().borrow_mut();
        }

        let handle = guard.get_slot(id).as_mut().unwrap();
        let mut component = handle.component.take().unwrap();

        // Drop guard for callback
        drop(guard);

        let item = callback(component.deref_mut());

        // Put component back
        let mut guard = self.runtime.local_component_store().borrow_mut();
        let handle = guard.get_slot(id).as_mut().unwrap();

        // There is nothing to drop, we own the component, so forget the `None` for better codegen
        std::mem::forget(handle.component.replace(component));

        Some(item)
    }

    #[cold]
    fn synchronize_component(&self, id: ComponentId, target_timestamp: Period) {
        let mut guard = self.runtime.local_component_store().borrow_mut();
        let handle = guard.get_slot(id).as_mut().unwrap();

        let Some(mut delta) = target_timestamp.checked_sub(handle.current_timestamp) else {
            return;
        };

        let mut current_timestamp = handle.current_timestamp;
        let mut component = handle.component.take().unwrap();

        if delta == Period::ZERO || !component.needs_work(&current_timestamp, &delta) {
            handle.component = Some(component);
            return;
        }
        drop(guard);

        loop {
            let mut last_attempted_allocation = Period::ZERO;
            let context = SynchronizationContext {
                runtime: self.runtime,
                current_timestamp: &mut current_timestamp,
                target_timestamp,
                last_attempted_allocation: &mut last_attempted_allocation,
            };
            component.synchronize(context);

            assert_ne!(
                last_attempted_allocation,
                Period::ZERO,
                "Synchronization attempt for component did not attempt to allocate time"
            );

            delta = target_timestamp - current_timestamp;
            let needs_work = component.needs_work(&current_timestamp, &delta);

            let mut guard = self.runtime.local_component_store().borrow_mut();
            let handle = guard.get_slot(id).as_mut().unwrap();

            handle.component = Some(component);
            handle.current_timestamp = current_timestamp;

            if needs_work {
                let hazard_timestamp = handle.current_timestamp + last_attempted_allocation;
                drop(guard);

                self.runtime
                    .machine()
                    .scheduler
                    .event_manager
                    .consume(*self, hazard_timestamp);

                // Re-acquire
                guard = self.runtime.local_component_store().borrow_mut();
                let handle = self.fetch_or_acquire_component(id, &mut guard);

                component = handle.component.take().unwrap();
                current_timestamp = handle.current_timestamp;
            } else {
                return;
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
        let thread = std::thread::current();

        loop {
            let Some((_, handle)) = self.data.global_component_store.remove_sync(&id) else {
                // Insert thread token
                self.data
                    .threads_awaiting_component
                    .entry_sync(id)
                    .or_default()
                    .entry(thread.id())
                    .or_insert(thread.clone());

                // Give components back so others can potentially access them
                self.release_all_components(local_component_store);

                // Await for that component to potentially become available
                std::thread::park();

                // Try again until we can acquire that component
                continue;
            };

            let slot = local_component_store.get_slot(id);
            *slot = Some(handle);

            // Clean up
            self.data
                .threads_awaiting_component
                .entry_sync(id)
                .or_default()
                .get_mut()
                .remove(&thread.id());

            return slot.as_mut().unwrap();
        }
    }

    /// Release all components currently available for releasing
    pub(crate) fn release_all_components(&self, local_component_store: &mut LocalComponentStore) {
        for (id, slot) in local_component_store.iter_mut() {
            // Check if the slot is occupied and the handle isn't borrowed
            if let Some(handle) = slot
                && handle.component.is_some()
            {
                let handle = slot.take().unwrap();

                self.data
                    .global_component_store
                    .insert_sync(id, handle)
                    .expect("Component shadowed by another component");

                let Some((_, threads_waiting)) =
                    self.data.threads_awaiting_component.remove_sync(&id)
                else {
                    continue;
                };

                // Wake those threads up
                for (_, thread) in threads_waiting {
                    thread.unpark();
                }
            }
        }
    }

    pub(crate) fn interact_all(
        &self,
        time: Period,
        mut callback: impl FnMut(&ComponentPath, &mut dyn Component),
    ) {
        for (path, id) in self.data.path2id.iter() {
            self.interact_dyn(*id, time, |component| callback(path, component))
                .unwrap()
        }
    }

    pub(crate) fn path_to_id(&self, path: &ComponentPath) -> Option<ComponentId> {
        self.data.path2id.get(path).copied()
    }

    pub(crate) fn get_timestamp<'b>(
        &'b self,
        id: impl Into<ComponentIdentifier<'b>>,
    ) -> Option<Period> {
        let id = match id.into() {
            ComponentIdentifier::Id(id) => id,
            ComponentIdentifier::Path(path) => self.data.path2id.get(path).copied()?,
        };

        let mut local_component_store_guard = self.runtime.local_component_store().borrow_mut();
        let handle = self.fetch_or_acquire_component(id, &mut local_component_store_guard);

        Some(handle.current_timestamp)
    }
}

pub enum ComponentIdentifier<'a> {
    Id(ComponentId),
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
        &mut self.0[id.0 as usize]
    }

    #[inline]
    fn iter_mut(&mut self) -> impl Iterator<Item = (ComponentId, &mut Option<ComponentHandle>)> {
        self.0
            .iter_mut()
            .enumerate()
            .map(|(id, component_handle)| (ComponentId(id as u32), component_handle))
    }
}
