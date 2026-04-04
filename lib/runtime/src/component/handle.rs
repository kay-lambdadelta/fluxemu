use crate::{
    component::{Component, ComponentVersion},
    handle::RuntimeApi,
    machine::builder::SchedulerParticipation,
    path::ComponentPath,
    scheduler::{Period, SynchronizationContext},
};

#[derive(Debug)]
struct SynchronizationData {
    /// Timestamp this component is actually updated to
    updated_timestamp: Period,
}

// NOTE: We add a generic here so this can be coerced unsized
#[derive(Debug)]
struct ComponentData<T: ?Sized> {
    synchronization_data: Option<SynchronizationData>,
    save_version: Option<ComponentVersion>,
    snapshot_version: Option<ComponentVersion>,
    path: ComponentPath,
    component: T,
}

// Store everything behind the box pointer so its easier to move around
#[derive(Debug)]
pub(crate) struct ComponentHandle(Box<ComponentData<dyn Component>>);

impl ComponentHandle {
    pub(crate) fn new(
        path: ComponentPath,
        scheduler_participation: Option<SchedulerParticipation>,
        save_version: Option<ComponentVersion>,
        snapshot_version: Option<ComponentVersion>,
        component: impl Component,
    ) -> Self {
        let synchronization_data = scheduler_participation.map(|_| SynchronizationData {
            updated_timestamp: Period::default(),
        });

        Self(Box::new(ComponentData {
            synchronization_data,
            save_version,
            snapshot_version,
            path,
            component,
        }))
    }

    #[inline]
    fn synchronize(&mut self, runtime: &RuntimeApi, time: Period) {
        if self.0.synchronization_data.is_none() {
            return;
        }

        let mut delta;
        let mut last_attempted_allocation = None;

        // Loop until the component is fully updated, processing events when relevant
        loop {
            let SynchronizationData { updated_timestamp } =
                self.0.synchronization_data.as_mut().unwrap();

            if *updated_timestamp < time {
                // Update delta in case something happened when we dropped and reacquired the lock
                delta = time - *updated_timestamp;

                // Check if the component is done or there is no allocated time
                if delta == Period::ZERO || !self.0.component.needs_work(delta) {
                    break;
                }

                let context = SynchronizationContext {
                    scheduler: &runtime.machine().scheduler,
                    updated_timestamp,
                    target_timestamp: time,
                    last_attempted_allocation: &mut last_attempted_allocation,
                };

                self.0.component.synchronize(context);

                // Prevent bad synchronization logic from spinning forever
                let last_attempted_allocation = last_attempted_allocation.take().expect(
                    "Synchronization attempt for component did not attempt to allocate time",
                );

                // Update delta
                delta = time - *updated_timestamp;

                // If the component yielded and there is still work, check events and try to run it again
                if self.0.component.needs_work(delta) {
                    // Try to consume any events that blocked this time allocation
                    let timestamp = *updated_timestamp + last_attempted_allocation;

                    // consume events
                    runtime
                        .machine()
                        .scheduler
                        .event_manager
                        .consume(self, runtime, timestamp);
                }
            } else {
                break;
            }
        }
    }

    /// Interact immutably with a component
    #[inline]
    pub fn interact<T>(
        &mut self,
        runtime: &RuntimeApi,
        time: Period,
        callback: impl FnOnce(&dyn Component) -> T,
    ) -> T {
        self.synchronize(runtime, time);

        callback(&self.0.component)
    }

    /// Interact mutably with a component
    #[inline]
    pub fn interact_mut<T>(
        &mut self,
        runtime: &RuntimeApi,
        time: Period,
        callback: impl FnOnce(&mut dyn Component) -> T,
    ) -> T {
        self.synchronize(runtime, time);

        callback(&mut self.0.component)
    }

    pub fn path(&self) -> &ComponentPath {
        &self.0.path
    }
}
