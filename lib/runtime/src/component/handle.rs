use std::{any::Any, fmt::Debug, sync::Arc};

use crate::{
    component::{Component, ComponentId, ComponentIdentifier},
    path::ComponentPath,
};

type Cast<T> = dyn for<'a> Fn(&'a mut dyn Component) -> &'a mut T + Send + Sync;

pub struct ComponentHandle<T: ?Sized> {
    id: ComponentId,
    path: ComponentPath,
    cast: Arc<Cast<T>>,
}

impl<T: ?Sized> ComponentHandle<T> {
    pub fn id(&self) -> ComponentId {
        self.id
    }

    pub fn path(&self) -> &ComponentPath {
        &self.path
    }

    pub(crate) fn cast<'a>(&self, component: &'a mut dyn Component) -> &'a mut T {
        (self.cast)(component)
    }
}

impl<C: Component> ComponentHandle<C> {
    pub(crate) fn new(id: ComponentId, path: ComponentPath) -> Self {
        Self {
            id,
            path,
            cast: Arc::new(downcast::<C>),
        }
    }

    pub fn as_interface<I: ?Sized + 'static>(
        &self,
        interface: fn(&mut C) -> &mut I,
    ) -> ComponentHandle<I> {
        ComponentHandle {
            id: self.id,
            path: self.path.clone(),
            cast: Arc::new(move |component: &mut dyn Component| {
                interface(downcast::<C>(component))
            }),
        }
    }
}

impl<T: ?Sized> Clone for ComponentHandle<T> {
    fn clone(&self) -> Self {
        Self {
            id: self.id,
            path: self.path.clone(),
            cast: self.cast.clone(),
        }
    }
}

impl<T: ?Sized> Debug for ComponentHandle<T> {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ComponentHandle")
            .field("id", &self.id)
            .field("path", &self.path)
            .finish_non_exhaustive()
    }
}

impl<'a, T: ?Sized> From<&'a ComponentHandle<T>> for ComponentIdentifier<'a> {
    fn from(handle: &'a ComponentHandle<T>) -> Self {
        ComponentIdentifier::Id(handle.id)
    }
}

fn downcast<C: Component>(component: &mut dyn Component) -> &mut C {
    (component as &mut dyn Any).downcast_mut::<C>().unwrap()
}
