use std::{fmt::Debug, sync::Mutex};

use fixed::{FixedU128, types::extra::U64};

use crate::{RuntimeHandle, component::ComponentRegistry, scheduler::queue::Queue};

pub mod event;
pub(crate) mod queue;
pub mod task;

#[derive(Debug, Default)]
pub struct Scheduler {
    pub queue: Queue,
    safe_advance_timestamp: Mutex<Period>,
    start_time: Period,
}

impl Scheduler {
    pub fn safe_advance_timestamp(&self) -> Period {
        *self.safe_advance_timestamp.lock().unwrap()
    }

    pub fn start_time(&self) -> Period {
        self.start_time
    }

    pub fn run(&self, component_registry: &ComponentRegistry<'_>, allocated_time: Period) {
        let target = self.safe_advance_timestamp() + allocated_time;

        self.queue
            .handle_deadlines_before(target, component_registry);

        let mut safe_advance_timestamp = self.safe_advance_timestamp.lock().unwrap();
        *safe_advance_timestamp = (*safe_advance_timestamp).max(target);
    }
}

/// Type representing a period, or an inverse frequency, as a [Q64.64](https://en.wikipedia.org/wiki/Q_(number_format))
pub type Period = FixedU128<U64>;

/// Type representing a frequency, or an inverse period, as a [Q64.64](https://en.wikipedia.org/wiki/Q_(number_format))
pub type Frequency = FixedU128<U64>;

/// Context to begin the synchronization process
#[derive(Debug)]
pub struct Context<'a> {
    pub(crate) runtime: &'a RuntimeHandle,
    pub(crate) current_timestamp: &'a mut Period,
    pub(crate) target_timestamp: Period,
    pub(crate) last_attempted_allocation: &'a mut Period,
}

impl<'a> Context<'a> {
    #[inline]
    pub fn runtime(&self) -> &'a RuntimeHandle {
        self.runtime
    }

    /// Create an iterator that continuously allocates an amount of time represented by period until either the target timestamp is reached
    /// or the runtime preempts the task
    #[inline]
    pub fn quanta_allocator<'b>(&'b mut self, period: Period) -> QuantaAllocator<'b, 'a> {
        let (last_seen_event_generation, budget) = self.check_allocation_preconditions(period);

        QuantaAllocator {
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
        let last_seen_event_generation = scheduler.queue.preemption_signal().generation();

        let mut stop_time = self.target_timestamp;
        if let Some(next_deadline) = scheduler.queue.next_deadline() {
            stop_time = stop_time.min(next_deadline);
        }

        let budget = (stop_time.saturating_sub(*self.current_timestamp) / period)
            .floor()
            .checked_to_num::<u32>()
            .unwrap_or(u32::MAX);

        (last_seen_event_generation, budget)
    }
}

/// Helper iterator to continuously allocate a period until the time budget is exhausted
pub struct QuantaAllocator<'b, 'a> {
    period: Period,
    budget: u32,
    last_seen_event_generation: u32,
    context: &'b mut Context<'a>,
}

impl<'b, 'a> Iterator for QuantaAllocator<'b, 'a> {
    type Item = Period;

    fn next(&mut self) -> Option<Self::Item> {
        let preemption_signal = self
            .context
            .runtime
            .machine()
            .scheduler
            .queue
            .preemption_signal();

        let current_generation = preemption_signal.generation();
        if current_generation != self.last_seen_event_generation {
            self.last_seen_event_generation = current_generation;
            self.rebudget();
        }

        if self.budget != 0 {
            self.budget -= 1;
        } else {
            std::hint::cold_path();

            return None;
        }

        let next_timestamp = *self.context.current_timestamp + self.period;
        *self.context.current_timestamp = next_timestamp;

        Some(next_timestamp)
    }
}

impl QuantaAllocator<'_, '_> {
    #[cold]
    fn rebudget(&mut self) {
        let mut stop_time = self.context.target_timestamp;

        // If a event exists, allow it to cut our budget short
        if let Some(next_event) = self
            .context
            .runtime
            .machine()
            .scheduler
            .queue
            .next_deadline()
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
