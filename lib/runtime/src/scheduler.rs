use std::{
    fmt::Debug,
    sync::{Arc, Mutex},
};

use fixed::{FixedU128, types::extra::U64};

use crate::{
    component::ComponentRegistry,
    event::{EventManager, PreemptionSignal},
    path::ComponentPath,
};

#[derive(Debug)]
pub(crate) struct Scheduler {
    pub event_manager: EventManager,
    pub current_driven_time: Mutex<Period>,
    driven: Vec<ComponentPath>,
    start_time: Period,
    preemption_signal: Arc<PreemptionSignal>,
}

impl Scheduler {
    pub fn new() -> Self {
        Scheduler {
            event_manager: EventManager::default(),
            driven: Vec::default(),
            current_driven_time: Mutex::default(),
            start_time: Period::default(),
            preemption_signal: Arc::new(PreemptionSignal::new()),
        }
    }

    pub fn now(&self) -> Period {
        *self.current_driven_time.lock().unwrap()
    }

    pub fn start_time(&self) -> Period {
        self.start_time
    }

    pub fn register_driven_component(&mut self, path: ComponentPath) {
        self.driven.push(path);
    }

    pub fn run(&self, component_registry: ComponentRegistry<'_>, allocated_time: Period) {
        let current_driven_time_guard = self.current_driven_time.lock().unwrap();
        let next_time = *current_driven_time_guard + allocated_time;
        drop(current_driven_time_guard);

        for path in &self.driven {
            component_registry.interact_dyn_mut(path, next_time, |_| {});
        }

        let mut current_driven_time_guard = self.current_driven_time.lock().unwrap();
        *current_driven_time_guard = next_time;
    }

    pub(crate) fn preemption_signal(&self) -> &Arc<PreemptionSignal> {
        &self.preemption_signal
    }
}

pub type Period = FixedU128<U64>;
pub type Frequency = FixedU128<U64>;

#[derive(Debug)]
pub struct SynchronizationContext<'a> {
    pub(crate) scheduler: &'a Scheduler,
    pub(crate) updated_timestamp: &'a mut Period,
    pub(crate) target_timestamp: Period,
    pub(crate) last_attempted_allocation: &'a mut Option<Period>,
}

impl<'a> SynchronizationContext<'a> {
    #[inline]
    pub fn allocate<'b>(&'b mut self, period: Period) -> QuantaIterator<'b, 'a> {
        *self.last_attempted_allocation = Some(period);

        let mut stop_time = self.target_timestamp;

        if let Some(next_event) = self.scheduler.event_manager.next_event() {
            stop_time = stop_time.min(next_event);
        }

        let budget = (stop_time.saturating_sub(*self.updated_timestamp) / period)
            .floor()
            .to_num::<u64>();

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
        let preemption_signal = self.context.scheduler.preemption_signal();

        // New event(s) spotted we have not evaluated
        while preemption_signal.needs_preemption() {
            let mut stop_time = self.context.target_timestamp;

            if let Some(next_event) = self.context.scheduler.event_manager.next_event() {
                stop_time = stop_time.min(next_event);
            }

            let new_budget = (stop_time.saturating_sub(*self.context.updated_timestamp)
                / self.period)
                .floor()
                .to_num::<u64>();

            self.budget = self.budget.min(new_budget);
        }

        if self.budget == 0 {
            return None;
        }
        self.budget -= 1;

        let next_timestamp = *self.context.updated_timestamp + self.period;
        *self.context.updated_timestamp = next_timestamp;
        Some(next_timestamp)
    }
}
