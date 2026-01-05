use std::{
    collections::HashMap,
    fmt::Debug,
    sync::{Arc, Mutex},
};

pub(crate) use event::{EventManager, EventType, PreemptionSignal, QueuedEvent};
use fixed::{FixedU128, types::extra::U64};
use rustc_hash::FxBuildHasher;

use crate::{component::ComponentHandle, path::FluxEmuPath};

mod event;
#[cfg(test)]
mod tests;
mod worker;

#[derive(Debug)]
pub struct DrivenComponent {
    component: ComponentHandle,
}

/// The main scheduler that the runtime uses to drive tasks
///
/// It is a frequency based cooperative scheduler with some optional out of
/// order execution stuff
#[derive(Debug)]
pub(crate) struct Scheduler {
    pub event_queue: Arc<EventManager>,
    driven: HashMap<FluxEmuPath, DrivenComponent, FxBuildHasher>,
    current_driven_time: Mutex<Period>,
    start_time: Period,
}

impl Scheduler {
    pub fn new() -> Self {
        Scheduler {
            event_queue: Arc::default(),
            driven: HashMap::default(),
            current_driven_time: Mutex::default(),
            start_time: Period::default(),
        }
    }

    pub fn now(&self) -> Period {
        *self.current_driven_time.lock().unwrap()
    }

    pub fn start_time(&self) -> Period {
        self.start_time
    }

    pub fn register_driven_component(&mut self, path: FluxEmuPath, component: ComponentHandle) {
        self.driven.insert(path, DrivenComponent { component });
    }

    pub fn run(&self, allocated_time: Period) {
        let mut current_driven_time_guard = self.current_driven_time.lock().unwrap();

        *current_driven_time_guard += allocated_time;
        let current_driven_time = *current_driven_time_guard;

        for driven in self.driven.values() {
            driven.component.interact_mut(current_driven_time, |_| {})
        }
    }
}

pub type Period = FixedU128<U64>;
pub type Frequency = FixedU128<U64>;

#[derive(Debug)]
pub struct SynchronizationContext<'a> {
    pub(crate) event_manager: &'a EventManager,
    pub(crate) updated_timestamp: &'a mut Period,
    pub(crate) target_timestamp: Period,
    pub(crate) last_attempted_allocation: &'a mut Option<Period>,
    pub(crate) interrupt: &'a PreemptionSignal,
}

impl<'a> SynchronizationContext<'a> {
    #[inline]
    pub fn allocate<'b>(
        &'b mut self,
        period: Period,
        execution_limit: Option<u64>,
    ) -> QuantaIterator<'b, 'a> {
        *self.last_attempted_allocation = Some(period);

        let mut stop_time = self.target_timestamp;

        if let Some(next_event) = self.event_manager.next_event() {
            stop_time = stop_time.min(next_event)
        }

        let mut budget = (stop_time.saturating_sub(*self.updated_timestamp) / period)
            .floor()
            .to_num::<u64>();

        if let Some(execution_limit) = execution_limit {
            budget = budget.min(execution_limit);
        }

        QuantaIterator {
            period,
            budget,
            context: self,
        }
    }
}

pub struct QuantaIterator<'b, 'a> {
    period: Period,
    budget: u64,
    context: &'b mut SynchronizationContext<'a>,
}

impl Iterator for QuantaIterator<'_, '_> {
    type Item = Period;

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        // New event(s) spotted we have not evaluated
        while self.context.interrupt.needs_preemption() {
            let mut stop_time = self.context.target_timestamp;

            if let Some(next_event) = self.context.event_manager.next_event() {
                stop_time = stop_time.min(next_event)
            }

            let new_budget = (stop_time.saturating_sub(*self.context.updated_timestamp)
                / self.period)
                .floor()
                .to_num::<u64>();

            self.budget = self.budget.min(new_budget);
        }

        if self.budget == 0 {
            return None;
        } else {
            self.budget -= 1;
        }

        let next_timestamp = *self.context.updated_timestamp + self.period;
        *self.context.updated_timestamp = next_timestamp;
        Some(next_timestamp)
    }
}
