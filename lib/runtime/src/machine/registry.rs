use std::{
    any::{Any, TypeId},
    collections::HashMap,
    fmt::Debug,
    sync::Arc,
};

use rustc_hash::FxBuildHasher;

use crate::{
    component::{Component, ComponentHandle, ComponentVersion, TypedComponentHandle},
    machine::builder::SchedulerParticipation,
    path::ComponentPath,
    scheduler::{Period, PreemptionSignal, SyncPointManager},
};

struct ComponentInfo {
    component: ComponentHandle,
    type_id: TypeId,
    save_version: Option<ComponentVersion>,
    snapshot_version: Option<ComponentVersion>,
}

impl Debug for ComponentInfo {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ComponentInfo").finish()
    }
}

#[derive(Debug)]
/// The store for components
pub struct ComponentRegistry
// This absolutely has to be thread-safe
where
    Self: Send + Sync,
{
    sync_point_manager: Arc<SyncPointManager>,
    components: HashMap<ComponentPath, ComponentInfo, FxBuildHasher>,
}

impl ComponentRegistry {
    pub fn new(sync_point_manager: Arc<SyncPointManager>) -> Self {
        Self {
            sync_point_manager,
            components: HashMap::default(),
        }
    }

    pub(crate) fn insert_component<C: Component>(
        &mut self,
        path: ComponentPath,
        scheduler_participation: SchedulerParticipation,
        preemption_signal: Arc<PreemptionSignal>,
        save_version: Option<ComponentVersion>,
        snapshot_version: Option<ComponentVersion>,
        component: C,
    ) {
        self.components.insert(
            path.clone(),
            ComponentInfo {
                component: ComponentHandle::new(
                    scheduler_participation,
                    Arc::downgrade(&self.sync_point_manager),
                    preemption_signal,
                    path,
                    component,
                ),
                type_id: TypeId::of::<C>(),
                save_version,
                snapshot_version,
            },
        );
    }

    #[inline]
    pub fn interact<C: Component, T>(
        &self,
        path: &ComponentPath,
        current_timestamp: Period,
        callback: impl FnOnce(&C) -> T,
    ) -> Option<T> {
        self.interact_dyn(path, current_timestamp, |component| {
            let component = (component as &dyn Any).downcast_ref::<C>().unwrap();
            callback(component)
        })
    }

    #[inline]
    pub fn interact_mut<C: Component, T>(
        &self,
        path: &ComponentPath,
        current_timestamp: Period,
        callback: impl FnOnce(&mut C) -> T,
    ) -> Option<T> {
        self.interact_dyn_mut(path, current_timestamp, |component| {
            let component = (component as &mut dyn Any).downcast_mut::<C>().unwrap();
            callback(component)
        })
    }

    #[inline]
    pub fn interact_dyn<T>(
        &self,
        path: &ComponentPath,
        current_timestamp: Period,
        callback: impl FnOnce(&dyn Component) -> T,
    ) -> Option<T> {
        let component_info = self.components.get(path)?;

        Some(
            component_info
                .component
                .interact(current_timestamp, |component| callback(component)),
        )
    }

    #[inline]
    pub fn interact_dyn_mut<T>(
        &self,
        path: &ComponentPath,
        current_timestamp: Period,
        callback: impl FnOnce(&mut dyn Component) -> T,
    ) -> Option<T> {
        let component_info = self.components.get(path)?;

        Some(
            component_info
                .component
                .interact_mut(current_timestamp, |component| callback(component)),
        )
    }

    pub fn typed_handle<C: Component>(
        &self,
        path: &ComponentPath,
    ) -> Option<TypedComponentHandle<C>> {
        let component_info = self.components.get(path)?;

        assert_eq!(component_info.type_id, TypeId::of::<C>());

        Some(unsafe { TypedComponentHandle::new(component_info.component.clone()) })
    }

    pub fn handle(&self, path: &ComponentPath) -> Option<ComponentHandle> {
        let component_info = self.components.get(path)?;

        Some(component_info.component.clone())
    }
}
