use std::{any::Any, collections::HashMap, fmt::Debug, thread::Thread};

use rustc_hash::FxBuildHasher;

use crate::{
    RuntimeApi,
    component::{Component, ComponentId, handle::ComponentHandle},
    path::ComponentPath,
    scheduler::Period,
};

struct ComponentInfo {
    component: Option<ComponentHandle>,
    threads_awaiting: Vec<Thread>,
}

impl Debug for ComponentInfo {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ComponentInfo").finish()
    }
}

#[derive(Debug, Default)]
/// The store for components
pub(crate) struct ComponentRegistryData {
    components: scc::HashMap<ComponentId, ComponentInfo, FxBuildHasher>,
    path2id: HashMap<ComponentPath, ComponentId, FxBuildHasher>,
}

impl ComponentRegistryData {
    pub(crate) fn insert_component(&mut self, path: ComponentPath, component: ComponentHandle) {
        let id = ComponentId::new();

        self.components
            .insert_sync(
                id,
                ComponentInfo {
                    component: Some(component),
                    threads_awaiting: Vec::default(),
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

    /// Release all components currently available for releasing
    pub(crate) fn unmitigate_components(&self, local_component_store: &mut LocalComponentStore) {
        for (id, handle) in local_component_store.drain() {
            // Fetch info
            let mut component_info = self.data.components.get_sync(&id).unwrap();

            // Put the component back
            component_info.component = Some(handle);

            // Get the threads that are waiting
            let threads_awaiting = std::mem::take(&mut component_info.threads_awaiting);

            // Unlock
            drop(component_info);

            // Wake those threads up
            for thread in threads_awaiting {
                thread.unpark();
            }
        }
    }

    #[inline]
    fn mitigate_component(&self, id: ComponentId) -> ComponentHandle {
        let mut local_component_store_guard = self.runtime.local_component_store().borrow_mut();
        let component_handle = local_component_store_guard.get_slot(id).take();
        drop(local_component_store_guard);

        if let Some(component_handle) = component_handle {
            return component_handle;
        }

        self.mitigate_from_global(id)
    }

    #[cold]
    fn mitigate_from_global(&self, id: ComponentId) -> ComponentHandle {
        let thread = std::thread::current();

        loop {
            let mut component_info_guard = self.data.components.get_sync(&id).unwrap();
            let component_handle = component_info_guard.component.take();

            let Some(component_handle) = component_handle else {
                component_info_guard.threads_awaiting.push(thread.clone());
                drop(component_info_guard);

                let mut local_component_store_guard =
                    self.runtime.local_component_store().borrow_mut();

                // Give components back so others can access them
                self.unmitigate_components(&mut local_component_store_guard);
                drop(local_component_store_guard);

                // Await for that component to potentially become available
                std::thread::park();

                // Try again
                continue;
            };
            drop(component_info_guard);

            return component_handle;
        }
    }

    #[inline]
    pub fn interact<C: Component, T>(
        &self,
        path: &ComponentPath,
        time: Period,
        callback: impl FnOnce(&C) -> T,
    ) -> Option<T> {
        self.interact_dyn(path, time, |component| {
            let component = (component as &dyn Any).downcast_ref::<C>().unwrap();
            callback(component)
        })
    }

    #[inline]
    pub fn interact_mut<C: Component, T>(
        &self,
        path: &ComponentPath,
        time: Period,
        callback: impl FnOnce(&mut C) -> T,
    ) -> Option<T> {
        self.interact_dyn_mut(path, time, |component| {
            let component = (component as &mut dyn Any).downcast_mut::<C>().unwrap();
            callback(component)
        })
    }

    #[inline]
    pub fn interact_dyn<'b, T>(
        &'b self,
        id: impl Into<ComponentIdentifier<'b>>,
        time: Period,
        callback: impl FnOnce(&dyn Component) -> T + 'b,
    ) -> Option<T> {
        let id = match id.into() {
            ComponentIdentifier::Id(id) => id,
            ComponentIdentifier::Path(path) => self.data.path2id.get(path).copied()?,
        };

        let mut component_handle = self.mitigate_component(id);
        let item = component_handle.interact(self.runtime, time, callback);

        let mut local_component_store_guard = self.runtime.local_component_store().borrow_mut();
        let entry = local_component_store_guard.get_slot(id);
        *entry = Some(component_handle);

        Some(item)
    }

    #[inline]
    pub fn interact_dyn_mut<'b, T>(
        &'b self,
        id: impl Into<ComponentIdentifier<'b>>,
        time: Period,
        callback: impl FnOnce(&mut dyn Component) -> T + 'b,
    ) -> Option<T> {
        let id = match id.into() {
            ComponentIdentifier::Id(id) => id,
            ComponentIdentifier::Path(path) => self.data.path2id.get(path).copied()?,
        };

        let mut component_handle = self.mitigate_component(id);
        let item = component_handle.interact_mut(self.runtime, time, callback);

        let mut local_component_store_guard = self.runtime.local_component_store().borrow_mut();
        let entry = local_component_store_guard.get_slot(id);
        *entry = Some(component_handle);

        Some(item)
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
    pub fn get_slot(&mut self, id: ComponentId) -> &mut Option<ComponentHandle> {
        let id = id.0 as usize;

        if id >= self.0.len() {
            self.0.resize_with(id + 1, || None);
        }

        &mut self.0[id]
    }

    pub fn drain(&mut self) -> impl Iterator<Item = (ComponentId, ComponentHandle)> {
        self.0
            .drain(..)
            .enumerate()
            .filter_map(|(id, component_handle)| Some((ComponentId(id as u32), component_handle?)))
    }
}
