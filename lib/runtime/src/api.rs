use std::{
    borrow::Cow,
    sync::{Arc, Weak},
};

use fluxemu_program::ProgramSpecification;

use crate::{
    component::{Component, ComponentHandle, TypedComponentHandle},
    graphics::GraphicsApi,
    machine::Machine,
    memory::{AddressSpace, AddressSpaceId, MemoryRemappingCommand},
    path::{ComponentPath, ResourcePath},
    scheduler::{EventType, Frequency, Period},
};

#[derive(Debug, Clone)]
pub struct RuntimeHandle(pub(crate) Weak<Machine>);

#[derive(Debug)]
pub struct Runtime(Arc<Machine>);

impl Runtime {
    pub fn address_space(&self, address_space_id: AddressSpaceId) -> Option<&AddressSpace> {
        self.0.address_space(address_space_id)
    }

    pub fn insert_sync_point(
        &self,
        time: Period,
        target_path: &ComponentPath,
        name: impl Into<Cow<'static, str>>,
    ) {
        let component = self.0.registry.handle(target_path).unwrap();

        self.0
            .scheduler
            .sync_point_manager
            .queue(component, time, EventType::Once, name.into());

        self.interrupt_in_flight_synchronization();
    }

    pub fn insert_sync_point_with_frequency(
        &self,
        time: Period,
        frequency: Frequency,
        target_path: &ComponentPath,
        name: impl Into<Cow<'static, str>>,
    ) {
        let component = self.0.registry.handle(target_path).unwrap();

        self.0.scheduler.sync_point_manager.queue(
            component,
            time,
            EventType::Repeating { frequency },
            name.into(),
        );

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
        callback: impl FnOnce(&mut G::Texture),
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
}

impl RuntimeHandle {
    pub fn get(&self) -> Runtime {
        let machine = self.0.upgrade().expect("Machine has been dropped");

        Runtime(machine)
    }
}
