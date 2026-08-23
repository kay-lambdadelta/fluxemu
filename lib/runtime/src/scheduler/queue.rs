use std::{
    cmp::Reverse,
    collections::BinaryHeap,
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
        let mut heap_guard = self.heap.lock().unwrap();

        while let Some(item) = heap_guard.peek() {
            if timestamp < item.deadline.0 {
                // The next item's deadline doesn't overlap with the specified period
                break;
            }

            let item = heap_guard.pop().unwrap();

            match item.type_ {
                Type::Event { path, mode, data } => {
                    if let EventMode::Repeating { frequency } = mode {
                        let next_deadline = item.deadline.0 + frequency.recip();

                        heap_guard.push(QueueItem {
                            deadline: Reverse(next_deadline),
                            type_: Type::Event {
                                path: path.clone(),
                                mode,
                                data: dyn_clone::clone_box(data.as_ref()),
                            },
                        });

                        self.preemption_signal.bump();
                    }

                    drop(heap_guard);

                    component_registry.interact_dyn(&path, &item.deadline.0, |component| {
                        component.handle_event(data);
                    });
                }
                Type::AlwaysModeTask { path } => {
                    drop(heap_guard);

                    component_registry.interact_dyn(
                        path.parent().unwrap(),
                        &item.deadline.0,
                        |_| {},
                    );
                }
            }

            heap_guard = self.heap.lock().unwrap();
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
    pub(crate) fn generation(&self) -> u32 {
        self.0.load(Ordering::Acquire)
    }
}
