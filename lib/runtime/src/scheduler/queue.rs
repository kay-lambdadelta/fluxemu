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
    heap: Mutex<BinaryHeap<QueueItem>>,
    preemption_signal: PreemptionSignal,
}

impl Queue {
    #[inline]
    pub fn reschedule_task(&self, path: ResourcePath, at: Period) {
        let item = QueueItem {
            deadline: Reverse(at),
            type_: Type::AlwaysModeTask { path },
        };

        self.heap.lock().unwrap().push(item);
    }

    #[inline]
    pub fn schedule_event(
        &self,
        path: ComponentPath,
        at: Period,
        mode: EventMode,
        data: Box<dyn Event>,
    ) {
        let item = QueueItem {
            deadline: Reverse(at),
            type_: Type::Event { path, mode, data },
        };

        self.heap.lock().unwrap().push(item);

        self.preemption_signal.bump();
    }

    #[inline]
    pub fn handle_deadlines_before(
        &self,
        timestamp: Period,
        component_registry: &ComponentRegistry<'_>,
    ) {
        loop {
            let mut heap_guard = self.heap.lock().unwrap();

            let Some(mut item) = heap_guard.peek_mut() else {
                break;
            };

            if timestamp < item.deadline.0 {
                // The next item's deadline doesn't overlap with the specified period
                break;
            }

            match &item.type_ {
                Type::Event { path, mode, data } => {
                    let (path, deadline, data) = if let EventMode::Repeating { frequency } = mode {
                        // Copy needed data
                        let deadline = item.deadline.0;
                        let data = dyn_clone::clone_box(data.as_ref());
                        let path = path.clone();

                        let next_deadline = deadline + frequency.recip();

                        // Assign to its next deadline
                        item.deadline = Reverse(next_deadline);
                        self.preemption_signal.bump();

                        // Drop to prevent deadlocks due to reentrancy
                        drop(item);
                        drop(heap_guard);

                        (path, deadline, data)
                    } else {
                        // Remove from queue
                        let item = PeekMut::pop(item);
                        let Type::Event { path, data, .. } = item.type_ else {
                            unreachable!()
                        };

                        // Drop to prevent deadlocks due to reentrancy
                        drop(heap_guard);

                        (path, item.deadline.0, data)
                    };

                    // Events need to stop *on the timestamp* because they represent a full halt to service some periodic happening
                    component_registry.interact_dyn(&path, &deadline, |component| {
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
                    drop(heap_guard);

                    // Run the tasks associated with said component
                    component_registry.interact_dyn(
                        path.parent().unwrap(),
                        // Bound by the timestamp, in order to facilitate batching
                        &timestamp,
                        |_| {},
                    );
                }
            }
        }
    }

    #[inline]
    pub fn next_deadline(&self) -> Option<Period> {
        let queue_guard = self.heap.lock().unwrap();

        if let Some(next_event) = queue_guard.peek() {
            return Some(next_event.deadline.0);
        }

        None
    }

    pub(crate) fn preemption_signal(&self) -> &PreemptionSignal {
        &self.preemption_signal
    }
}

#[derive(Debug)]
struct QueueItem {
    deadline: Reverse<Period>,
    type_: Type,
}

impl PartialEq for QueueItem {
    fn eq(&self, other: &Self) -> bool {
        self.deadline.eq(&other.deadline)
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
        self.deadline.cmp(&other.deadline)
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
