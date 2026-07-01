use std::{fmt::Debug, sync::Mutex};

use fixed::{FixedU128, types::extra::U64};

use crate::{
    RuntimeApi,
    component::ComponentRegistry,
    event::{EventManager, EventPreemptionSignal},
    machine::Machine,
    path::ComponentPath,
};

#[derive(Debug)]
pub(crate) struct Scheduler {
    pub event_manager: EventManager,
    safe_advance_timestamp: Mutex<Period>,
    driven: Vec<ComponentPath>,
    start_time: Period,
    event_preemption_signal: EventPreemptionSignal,
}

impl Scheduler {
    pub fn new() -> Self {
        Scheduler {
            event_manager: EventManager::default(),
            driven: Vec::default(),
            safe_advance_timestamp: Mutex::default(),
            start_time: Period::default(),
            event_preemption_signal: EventPreemptionSignal::new(),
        }
    }

    /// Retrieves the latest timestamp that the machine has been driven to
    pub fn safe_advance_timestamp(&self) -> Period {
        *self.safe_advance_timestamp.lock().unwrap()
    }

    pub fn start_time(&self) -> Period {
        self.start_time
    }

    /// Register a new component that is directly driven by the scheduler
    ///
    /// For machine builder purposes
    pub fn register_driven_component(&mut self, path: ComponentPath) {
        self.driven.push(path);
    }

    /// Run the scheduler for a given amount of time, advancing the machine's timestamp and interacting with driven components
    ///
    /// After all driven components (ie: cpus) are successfully advanced, the safe advance timestamp is updated to reflect the new time
    pub fn run(&self, component_registry: ComponentRegistry<'_>, allocated_time: Period) {
        // Grab current time
        let safe_advance_timestamp = self.safe_advance_timestamp() + allocated_time;

        // Advance the time forward for all driven components
        for path in &self.driven {
            component_registry.interact_dyn(path, safe_advance_timestamp, |_| {});
        }

        // Set the new time, marking that the machine has officially advanced to this time
        let mut safe_advance_timestamp_guard = self.safe_advance_timestamp.lock().unwrap();
        *safe_advance_timestamp_guard = (*safe_advance_timestamp_guard).max(safe_advance_timestamp);
    }

    /// The preemption signal causes at least one [QuantaIterator] to stop active work and service events
    pub fn preemption_signal(&self) -> &EventPreemptionSignal {
        &self.event_preemption_signal
    }
}

/// Type representing a period, or a inverse frequency, as a Q64.64
pub type Period = FixedU128<U64>;
/// Type representing a frequency, or a inverse period, as a Q64.64
pub type Frequency = FixedU128<U64>;

/// Context to begin the synchronization process
#[derive(Debug)]
pub struct SynchronizationContext<'a> {
    pub(crate) runtime: RuntimeApi<&'a Machine>,
    pub(crate) current_timestamp: &'a mut Period,
    pub(crate) target_timestamp: Period,
    pub(crate) last_attempted_allocation: &'a mut Period,
}

impl<'a> SynchronizationContext<'a> {
    /// Convenience method to get a [`ComponentRuntimeApi`] by borrowing from this context
    #[inline]
    pub fn runtime(&self) -> RuntimeApi<&'a Machine> {
        self.runtime.clone()
    }

    /// Create an iterator that continuously allocates an amount of time represented by period until either the target timestamp is reached
    /// or the runtime preempts the task
    #[inline]
    pub fn allocate_continuous<'b>(&'b mut self, period: Period) -> QuantaIterator<'b, 'a> {
        let (last_seen_event_generation, budget) = self.check_allocation_preconditions(period);

        QuantaIterator {
            period,
            budget,
            last_seen_event_generation,
            context: self,
        }
    }

    #[inline]
    fn check_allocation_preconditions(&mut self, period: Period) -> (u32, u32) {
        assert_ne!(period, Period::ZERO, "Cannot allocate zero period");
        *self.last_attempted_allocation = period;

        let scheduler = &self.runtime.machine().scheduler;
        let last_seen_event_generation = scheduler.preemption_signal().generation();

        let mut stop_time = self.target_timestamp;
        if let Some(next_event) = scheduler.event_manager.next_event() {
            stop_time = stop_time.min(next_event);
        }

        let budget = (stop_time.saturating_sub(*self.current_timestamp) / period)
            .floor()
            .checked_to_num::<u32>()
            .unwrap_or(u32::MAX);

        (last_seen_event_generation, budget)
    }
}

/// Helper iterator to continuously allocate a period until the time budget is exhausted
pub struct QuantaIterator<'b, 'a> {
    period: Period,
    budget: u32,
    last_seen_event_generation: u32,
    context: &'b mut SynchronizationContext<'a>,
}

impl Iterator for QuantaIterator<'_, '_> {
    type Item = Period;

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        let preemption_signal = self.context.runtime.machine().scheduler.preemption_signal();

        let current_generation = preemption_signal.generation();
        if current_generation != self.last_seen_event_generation {
            self.last_seen_event_generation = current_generation;
            self.rebudget();
        }

        if self.budget == 0 {
            std::hint::cold_path();

            return None;
        }
        self.budget -= 1;

        *self.context.current_timestamp += self.period;

        Some(*self.context.current_timestamp)
    }
}

impl<'b, 'a> QuantaIterator<'b, 'a> {
    #[cold]
    fn rebudget(&mut self) {
        let mut stop_time = self.context.target_timestamp;

        // If a event exists, allow it to cut our budget short
        if let Some(next_event) = self
            .context
            .runtime
            .machine()
            .scheduler
            .event_manager
            .next_event()
        {
            stop_time = stop_time.min(next_event);
        }

        // Recalculate budget
        let new_budget = (stop_time.saturating_sub(*self.context.current_timestamp) / self.period)
            .floor()
            .checked_to_num()
            .unwrap_or(u32::MAX);

        self.budget = self.budget.min(new_budget);
    }
}
