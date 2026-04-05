use std::{any::Any, collections::HashMap, fmt::Debug, thread::Thread};

use rustc_hash::FxBuildHasher;

use crate::{
    RuntimeApi,
    component::{Component, ComponentId, handle::ComponentHandle},
    machine::{RUNTIME_CONTEXT, RuntimeCurrentThreadContext},
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
    pub(crate) fn unmitigate_components(&self, context: &mut RuntimeCurrentThreadContext) {
        for (index, handle) in context.local_component_store.drain(..).enumerate() {
            if let Some(handle) = handle {
                // Get ID
                let id = ComponentId(index as u32);

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
    }

    #[inline]
    fn mitigate_component(&self, id: ComponentId) -> ComponentHandle {
        let component_handle = RUNTIME_CONTEXT.with_borrow_mut(
            #[inline]
            |runtime_context| {
                let runtime_context = runtime_context.as_mut().unwrap();

                get_or_initialize_default(id, &mut runtime_context.local_component_store).take()
            },
        );

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

                // Release all components we can while blocked
                RUNTIME_CONTEXT.with_borrow_mut(|runtime_context| {
                    let runtime_context = runtime_context.as_mut().unwrap();

                    self.unmitigate_components(runtime_context);
                });

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

        RUNTIME_CONTEXT.with_borrow_mut(
            #[inline]
            |runtime_context| {
                let runtime_context = runtime_context.as_mut().unwrap();

                let entry =
                    get_or_initialize_default(id, &mut runtime_context.local_component_store);

                *entry = Some(component_handle);
            },
        );

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

        RUNTIME_CONTEXT.with_borrow_mut(
            #[inline]
            |runtime_context| {
                let runtime_context = runtime_context.as_mut().unwrap();

                let entry =
                    get_or_initialize_default(id, &mut runtime_context.local_component_store);

                *entry = Some(component_handle);
            },
        );

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

#[inline]
fn get_or_initialize_default(
    id: ComponentId,
    store: &mut Vec<Option<ComponentHandle>>,
) -> &mut Option<ComponentHandle> {
    let id = id.0 as usize;

    if id >= store.len() {
        store.resize_with(id + 1, || None);
    }

    &mut store[id]
}
