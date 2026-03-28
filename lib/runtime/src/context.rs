use std::{borrow::Cow, cell::RefCell, collections::HashMap, sync::Arc};

use fluxemu_program::ProgramSpecification;
use guardian::ArcMutexGuardian;
use rustc_hash::FxBuildHasher;

use crate::{
    component::{Component, ComponentHandle, EventType, HandleInner, TypedComponentHandle},
    graphics::GraphicsApi,
    machine::Machine,
    memory::{AddressSpace, AddressSpaceId, MemoryRemappingCommand},
    path::{ComponentPath, ResourcePath},
    scheduler::{EventManager, EventRequeueMode, Period},
};

#[derive(Debug)]
pub struct RuntimeHandle(Arc<Machine>);

impl RuntimeHandle {
    #[inline]
    pub fn current() -> Self {
        RuntimeCurrentThreadContext::interact(|context| {
            RuntimeHandle(context.current_machine.clone())
        })
    }

    pub fn address_space(&self, address_space_id: AddressSpaceId) -> Option<&AddressSpace> {
        self.0.address_space(address_space_id)
    }

    pub fn insert_event(
        &self,
        name: impl Into<Cow<'static, str>>,
        time: Period,
        target_path: &ComponentPath,
        requeue_mode: EventRequeueMode,
        data: EventType,
    ) {
        let component = self.0.registry.handle(target_path).unwrap();

        self.0
            .scheduler
            .event_manager
            .queue(name.into(), time, component, requeue_mode, data);

        self.interrupt_in_flight_synchronization();
    }

    pub fn remap_address_space(
        &self,
        address_space_id: AddressSpaceId,
        commands: impl IntoIterator<Item = MemoryRemappingCommand>,
    ) {
        let address_space = &self
            .address_space(address_space_id)
            .expect("Unknown address space");

        address_space.remap(commands, &self.0.registry);
    }

    pub fn interact<C: Component, T>(
        &self,
        path: &ComponentPath,
        callback: impl FnOnce(&C) -> T,
    ) -> Option<T> {
        self.0.interact(path, callback)
    }

    pub fn interact_mut<C: Component, T: 'static>(
        &self,
        path: &ComponentPath,
        callback: impl FnOnce(&mut C) -> T,
    ) -> Option<T> {
        self.0.interact_mut(path, callback)
    }

    pub fn interact_dyn<T>(
        &self,
        path: &ComponentPath,
        callback: impl FnOnce(&dyn Component) -> T,
    ) -> Option<T> {
        self.0.interact_dyn(path, callback)
    }

    pub fn interact_dyn_mut<T>(
        &self,
        path: &ComponentPath,
        callback: impl FnOnce(&mut dyn Component) -> T,
    ) -> Option<T> {
        self.0.interact_dyn_mut(path, callback)
    }

    pub fn component_handle(&self, path: &ComponentPath) -> Option<ComponentHandle> {
        self.0.component_handle(path)
    }

    pub fn typed_component_handle<C: Component>(
        &self,
        path: &ComponentPath,
    ) -> Option<TypedComponentHandle<C>> {
        self.0.typed_component_handle(path)
    }

    pub fn commit_framebuffer<G: GraphicsApi>(
        &self,
        path: &ResourcePath,
        callback: impl FnOnce(&mut G::Framebuffer),
    ) {
        let mut framebuffer_guard = self
            .0
            .framebuffers
            .get(path)
            .expect("Could not find framebuffer")
            .lock()
            .unwrap();

        callback(
            framebuffer_guard
                .downcast_mut()
                .expect("This item is not a valid framebuffer"),
        )
    }

    pub fn program_specification(&self) -> Option<&ProgramSpecification> {
        self.0.program_specification()
    }

    fn interrupt_in_flight_synchronization(&self) {
        for signal in &self.0.preemption_signals {
            signal.event_occured();
        }
    }

    pub(crate) fn sync_point_manager(&self) -> &EventManager {
        &self.0.scheduler.event_manager
    }
}

thread_local! {
    static RUNTIME_CURRENT_THREAD_CONTEXT: RefCell<Option<RuntimeCurrentThreadContext>> = RefCell::default();
}

pub(crate) struct RuntimeCurrentThreadContext {
    pub guard_cache: HashMap<usize, ArcMutexGuardian<HandleInner<dyn Component>>, FxBuildHasher>,
    pub current_machine: Arc<Machine>,
}

impl RuntimeCurrentThreadContext {
    #[inline]
    pub fn enter<T>(machine: Arc<Machine>, callback: impl FnOnce() -> T) -> T {
        RUNTIME_CURRENT_THREAD_CONTEXT.with(|context| {
            let mut context_guard = context.borrow_mut();

            if context_guard.is_some() {
                panic!("Reentrancy on the runtime is not allowed");
            }

            *context_guard = Some(RuntimeCurrentThreadContext {
                current_machine: machine,
                guard_cache: HashMap::default(),
            });
            drop(context_guard);

            let item = callback();

            // Unset context
            let mut context_guard = context.borrow_mut();
            *context_guard = None;

            item
        })
    }

    #[inline]
    pub fn interact<T>(callback: impl FnOnce(&RuntimeCurrentThreadContext) -> T) -> T {
        RUNTIME_CURRENT_THREAD_CONTEXT.with(|context| {
            let context_guard = context.borrow();

            if let Some(context) = context_guard.as_ref() {
                callback(context)
            } else {
                panic!("Not inside of runtime context");
            }
        })
    }

    #[inline]
    pub fn interact_mut<T>(callback: impl FnOnce(&mut RuntimeCurrentThreadContext) -> T) -> T {
        RUNTIME_CURRENT_THREAD_CONTEXT.with(|context| {
            let mut context_guard = context.borrow_mut();

            if let Some(context) = context_guard.as_mut() {
                callback(context)
            } else {
                panic!("Not inside of runtime context");
            }
        })
    }
}
