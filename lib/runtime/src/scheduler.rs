use std::{fmt::Debug, sync::Mutex};

use fixed::{FixedU128, types::extra::U64};

use crate::{
    component::ComponentRegistry,
    event::{EventManager, PreemptionSignal},
    path::ComponentPath,
};

#[derive(Debug)]
pub(crate) struct Scheduler {
    pub event_manager: EventManager,
    current_driven_time: Mutex<Period>,
    driven: Vec<ComponentPath>,
    start_time: Period,
    preemption_signal: PreemptionSignal,
}

impl Scheduler {
    pub fn new() -> Self {
        Scheduler {
            event_manager: EventManager::default(),
            driven: Vec::default(),
            current_driven_time: Mutex::default(),
            start_time: Period::default(),
            preemption_signal: PreemptionSignal::new(),
        }
    }

    /// Retrieves the latest timestamp that the machine has been driven to
    pub fn now(&self) -> Period {
        *self.current_driven_time.lock().unwrap()
    }

    pub fn start_time(&self) -> Period {
        self.start_time
    }

    /// Register a new component that is directly driven by the scheduler
    ///
    /// Ment for machine builder purposes
    pub fn register_driven_component(&mut self, path: ComponentPath) {
        self.driven.push(path);
    }

    pub fn run(&self, component_registry: ComponentRegistry<'_>, allocated_time: Period) {
        // Grab current time
        let next_time = self.now() + allocated_time;

        // Advance the time forward for all driven components
        for path in &self.driven {
            component_registry.interact_dyn_mut(path, next_time, |_| {});
        }

        // Set the new time, marking that the machine has officially advanced to this time
        let mut current_driven_time_guard = self.current_driven_time.lock().unwrap();
        *current_driven_time_guard = next_time;
    }

    /// The preemption signal causes at least one [QuantaIterator] to stop active work and service events
    pub fn preemption_signal(&self) -> &PreemptionSignal {
        &self.preemption_signal
    }
}

/// Type representing a period, or a inverse frequency, as a Q64.64
pub type Period = FixedU128<U64>;
/// Type representing a frequency, or a inverse period, as a Q64.64
pub type Frequency = FixedU128<U64>;

/// Context to begin the synchronization process
#[derive(Debug)]
pub struct SynchronizationContext<'a> {
    pub(crate) scheduler: &'a Scheduler,
    pub(crate) current_timestamp: &'a mut Period,
    pub(crate) target_timestamp: Period,
    pub(crate) last_attempted_allocation: &'a mut Option<Period>,
}

impl<'a> SynchronizationContext<'a> {
    /// Create an iterator that continously allocates an amount of time represented by period until either the target timestamp is reached
    /// or the runtime preempts the task
    #[inline]
    pub fn allocate<'b>(&'b mut self, period: Period) -> QuantaIterator<'b, 'a> {
        *self.last_attempted_allocation = Some(period);

        let mut stop_time = self.target_timestamp;

        if let Some(next_event) = self.scheduler.event_manager.next_event() {
            stop_time = stop_time.min(next_event);
        }

        let budget = (stop_time.saturating_sub(*self.current_timestamp) / period)
            .floor()
            .to_num::<u64>();

        QuantaIterator {
            period,
            budget,
            timestamp_at_allocation: *self.current_timestamp,
            steps_taken: 0,
            context: self,
        }
    }
}

/// Helper iterator to continously allocate a period until the time budget is exhausted
pub struct QuantaIterator<'b, 'a> {
    period: Period,
    budget: u64,
    timestamp_at_allocation: Period,
    steps_taken: u64,
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

            // If a event exists, allow it to cut our budget short
            if let Some(next_event) = self.context.scheduler.event_manager.next_event() {
                stop_time = stop_time.min(next_event);
            }

            // Recalculate budget
            let new_budget = (stop_time.saturating_sub(*self.context.current_timestamp)
                / self.period)
                .floor()
                .to_num::<u64>();

            self.budget = self.budget.min(new_budget);
        }

        if self.budget == 0 {
            return None;
        }
        self.budget -= 1;
        self.steps_taken += 1;

        // Multiply by steps taken to reduce accumulated error
        let next_timestamp =
            self.timestamp_at_allocation + self.period * FixedU128::from(self.steps_taken);
        *self.context.current_timestamp = next_timestamp;

        // Return new now
        Some(next_timestamp)
    }
}
