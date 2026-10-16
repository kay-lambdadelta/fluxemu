use std::{
    cmp::Reverse,
    collections::{BinaryHeap, binary_heap::PeekMut},
    sync::{
        Mutex,
        atomic::{AtomicU32, Ordering},
    },
};

use crate::{
    ComponentPath,
    component::ComponentRegistry,
    path::ResourcePath,
    scheduler::{
        Period,
        event::{Event, EventMode},
    },
};

#[derive(Debug, Default)]
pub struct Queue {
    state: Mutex<QueueState>,
    preemption_signal: PreemptionSignal,
}

#[derive(Debug, Default)]
struct QueueState {
    heap: BinaryHeap<QueueItem>,
    next_sequence: u64,
}

impl QueueState {
    fn push(&mut self, deadline: Period, type_: Type) {
        let sequence = self.take_sequence();

        self.heap.push(QueueItem {
            key: Reverse((deadline, sequence)),
            type_,
        });
    }

    fn take_sequence(&mut self) -> u64 {
        let sequence = self.next_sequence;
        self.next_sequence += 1;

        sequence
    }
}

impl Queue {
    #[inline]
    pub fn reschedule_task(&self, path: ResourcePath, at: Period) {
        self.state
            .lock()
            .unwrap()
            .push(at, Type::AlwaysModeTask { path });
    }

    #[inline]
    pub fn schedule_event(
        &self,
        path: ComponentPath,
        at: Period,
        mode: EventMode,
        data: Box<dyn Event>,
    ) {
        self.state
            .lock()
            .unwrap()
            .push(at, Type::Event { path, mode, data });

        self.preemption_signal.bump();
    }

    #[inline]
    pub fn handle_deadlines_until(
        &self,
        timestamp: Period,
        component_registry: ComponentRegistry<'_>,
    ) {
        loop {
            let mut state_guard = self.state.lock().unwrap();
            let next_sequence = state_guard.take_sequence();

            let Some(mut item) = state_guard.heap.peek_mut() else {
                break;
            };

            if timestamp < item.deadline() {
                // The next item's deadline doesn't overlap with the specified period
                break;
            }

            match &item.type_ {
                Type::Event { path, mode, data } => {
                    let (path, deadline, data) = if let EventMode::Repeating { frequency } = mode {
                        // Copy needed data
                        let deadline = item.deadline();
                        let data = dyn_clone::clone_box(data.as_ref());
                        let path = path.clone();

                        let next_deadline = deadline + frequency.recip();

                        // Assign to its next deadline
                        item.key = Reverse((next_deadline, next_sequence));
                        self.preemption_signal.bump();

                        // Drop to prevent deadlocks due to reentrancy
                        drop(item);
                        drop(state_guard);

                        (path, deadline, data)
                    } else {
                        // Remove from queue
                        let item = PeekMut::pop(item);
                        let deadline = item.deadline();
                        let Type::Event { path, data, .. } = item.type_ else {
                            unreachable!()
                        };

                        // Drop to prevent deadlocks due to reentrancy
                        drop(state_guard);

                        (path, deadline, data)
                    };

                    // Events need to stop *on the timestamp* because they represent a full halt to service some periodic happening
                    component_registry.interact_dyn(&path, deadline, |component| {
                        component.handle_event(data);
                    });
                }
                // Always tasks do not need to be stopped on the dot however, and it would make it more efficient if they
                // ran for as long runs as possible.
                Type::AlwaysModeTask { .. } => {
                    // Remove from queue
                    let item = PeekMut::pop(item);
                    let Type::AlwaysModeTask { path } = item.type_ else {
                        unreachable!()
                    };

                    // Drop to prevent deadlocks due to reentrancy
                    drop(state_guard);

                    // Run the tasks associated with said component
                    component_registry.interact_dyn(
                        path.parent().unwrap(),
                        // Bound by the timestamp, in order to facilitate batching
                        timestamp,
                        |_| {},
                    );
                }
            }
        }
    }

    #[inline]
    pub fn next_deadline(&self) -> Option<Period> {
        self.state
            .lock()
            .unwrap()
            .heap
            .peek()
            .map(QueueItem::deadline)
    }

    pub(crate) fn preemption_signal(&self) -> &PreemptionSignal {
        &self.preemption_signal
    }
}

#[derive(Debug)]
struct QueueItem {
    key: Reverse<(Period, u64)>,
    type_: Type,
}

impl QueueItem {
    #[inline]
    fn deadline(&self) -> Period {
        self.key.0.0
    }
}

impl PartialEq for QueueItem {
    fn eq(&self, other: &Self) -> bool {
        self.key.eq(&other.key)
    }
}
impl Eq for QueueItem {}
impl PartialOrd for QueueItem {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}
impl Ord for QueueItem {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.key.cmp(&other.key)
    }
}

#[derive(Debug)]
enum Type {
    Event {
        path: ComponentPath,
        mode: EventMode,
        data: Box<dyn Event>,
    },
    AlwaysModeTask {
        path: ResourcePath,
    },
}

#[derive(Debug, Default)]
pub struct PreemptionSignal(AtomicU32);

impl PreemptionSignal {
    fn bump(&self) {
        self.0.fetch_add(1, Ordering::AcqRel);
    }

    #[inline]
    pub(crate) fn generation(&self) -> Generation {
        Generation(self.0.load(Ordering::Acquire))
    }
}

#[derive(Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
pub struct Generation(u32);
