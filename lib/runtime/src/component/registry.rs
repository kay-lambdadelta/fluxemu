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
    component::{Component, ComponentId, ComponentVersion},
    path::ComponentPath,
    scheduler::{Period, SynchronizationContext},
};

#[derive(Debug)]
pub(crate) struct ComponentHandle {
    current_timestamp: Period,
    synchronize: bool,
    #[allow(unused)]
    save_version: Option<ComponentVersion>,
    #[allow(unused)]
    snapshot_version: Option<ComponentVersion>,
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
    pub(crate) fn insert_component<C: Component>(
        &mut self,
        path: ComponentPath,
        save_version: Option<ComponentVersion>,
        snapshot_version: Option<ComponentVersion>,
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
    fn mitigate_component<'b>(
        &self,
        id: ComponentId,
        local_component_store: &'b mut LocalComponentStore,
    ) -> &'b mut ComponentHandle {
        if local_component_store.get_slot(id).is_some() {
            let component_handle = local_component_store.get_slot(id);

            component_handle.as_mut().unwrap()
        } else {
            self.mitigate_from_global(id, local_component_store)
        }
    }

    #[cold]
    fn mitigate_from_global<'b>(
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
                self.unmitigate_all_components(local_component_store);

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

    #[inline]
    pub fn interact<C: Component, T>(
        &self,
        path: &ComponentPath,
        time: Period,
        callback: impl FnOnce(&mut C) -> T,
    ) -> Option<T> {
        self.interact_dyn(
            path,
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
        callback: impl FnOnce(&mut dyn Component) -> T + 'b,
    ) -> Option<T> {
        let id = match id.into() {
            ComponentIdentifier::Id(id) => id,
            ComponentIdentifier::Path(path) => self.data.path2id.get(path).copied()?,
        };

        let mut local_component_store_guard = self.runtime.local_component_store().borrow_mut();
        let mut handle = self.mitigate_component(id, &mut local_component_store_guard);

        let mut last_attempted_allocation = None;

        if handle.synchronize {
            while let Some(mut delta) = target_timestamp.checked_sub(handle.current_timestamp) {
                // Copy out timestamp
                let mut current_timestamp = handle.current_timestamp;

                // Move out component
                let mut component = handle.component.take().unwrap();

                // Check to see if the component needs work
                if delta == Period::ZERO || !component.needs_work(delta) {
                    handle.component = Some(component);

                    break;
                }

                // Drop guard and synchronize
                drop(local_component_store_guard);
                let context = SynchronizationContext {
                    runtime: self.runtime,
                    current_timestamp: &mut current_timestamp,
                    target_timestamp,
                    last_attempted_allocation: &mut last_attempted_allocation,
                };
                component.synchronize(context);

                // Prevent bad synchronization logic from spinning forever
                let last_attempted_allocation = last_attempted_allocation.take().expect(
                    "Synchronization attempt for component did not attempt to allocate time",
                );

                // Update delta
                delta = target_timestamp - current_timestamp;

                // Check if the component needs more work
                let needs_work = component.needs_work(delta);

                // Reborrow everything
                local_component_store_guard = self.runtime.local_component_store().borrow_mut();
                handle = local_component_store_guard.get_slot(id).as_mut().unwrap();

                // Put back the component and its timestamp
                handle.component = Some(component);
                handle.current_timestamp = current_timestamp;

                // If the component yielded and there is still work, check events and try to run it again
                if needs_work {
                    // Calculate when the events need to be consumed in the future
                    let event_hazard_timestamp =
                        handle.current_timestamp + last_attempted_allocation;

                    // Drop guard
                    drop(local_component_store_guard);

                    // Try to consume any events that blocked this time allocation
                    self.runtime
                        .machine()
                        .scheduler
                        .event_manager
                        .consume(*self, event_hazard_timestamp);

                    // Reborrow everything
                    local_component_store_guard = self.runtime.local_component_store().borrow_mut();
                    handle = self.mitigate_component(id, &mut local_component_store_guard);
                } else {
                    break;
                }
            }
        } else {
            handle.current_timestamp = target_timestamp;
        }

        let handle = self.mitigate_component(id, &mut local_component_store_guard);
        let mut component = handle.component.take().unwrap();
        drop(local_component_store_guard);

        let item = callback(component.deref_mut());

        // Put component back
        let mut local_component_store_guard = self.runtime.local_component_store().borrow_mut();
        let slot = local_component_store_guard.get_slot(id);
        slot.as_mut().unwrap().component = Some(component);

        Some(item)
    }

    /// Release all components currently available for releasing
    pub(crate) fn unmitigate_all_components(
        &self,
        local_component_store: &mut LocalComponentStore,
    ) {
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

    /// Get the current timestamp of a component
    ///
    /// This may be before the current time if that component is currently synchronizing
    ///
    /// It is recommended this call is only used for fetching the current timestamp of your component, outside of any `synchronize` path of
    /// that component
    pub fn current_timestamp<'b>(
        &'b self,
        id: impl Into<ComponentIdentifier<'b>>,
    ) -> Option<Period> {
        let id = match id.into() {
            ComponentIdentifier::Id(id) => id,
            ComponentIdentifier::Path(path) => self.data.path2id.get(path).copied()?,
        };

        let mut local_component_store_guard = self.runtime.local_component_store().borrow_mut();
        let handle = self.mitigate_component(id, &mut local_component_store_guard);

        Some(handle.current_timestamp)
    }

    pub(crate) fn path_to_id(&self, path: &ComponentPath) -> Option<ComponentId> {
        self.data.path2id.get(path).copied()
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

#[derive(Debug, Default)]
pub(crate) struct LocalComponentStore(Vec<Option<ComponentHandle>>);

impl LocalComponentStore {
    #[inline]
    fn get_slot(&mut self, id: ComponentId) -> &mut Option<ComponentHandle> {
        let id = id.0 as usize;

        if id >= self.0.len() {
            self.0.resize_with(id + 1, || None);
        }

        &mut self.0[id]
    }

    #[inline]
    fn iter_mut(&mut self) -> impl Iterator<Item = (ComponentId, &mut Option<ComponentHandle>)> {
        self.0
            .iter_mut()
            .enumerate()
            .map(|(id, component_handle)| (ComponentId(id as u32), component_handle))
    }
}
