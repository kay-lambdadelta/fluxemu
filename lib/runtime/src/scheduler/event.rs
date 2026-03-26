use std::{
    borrow::Cow,
    cmp::Reverse,
    collections::BinaryHeap,
    fmt::Debug,
    sync::{
        Mutex,
        atomic::{AtomicBool, Ordering},
    },
};

use serde::{Deserialize, Serialize};

use crate::{
    component::{ComponentHandle, EventType},
    scheduler::{Frequency, Period},
};

#[derive(Debug, Default)]
pub struct EventManager {
    heap: Mutex<BinaryHeap<QueuedEvent>>,
}

impl EventManager {
    pub(crate) fn queue(
        &self,
        name: Cow<'static, str>,
        time: Period,
        component: ComponentHandle,
        requeue_mode: EventRequeueMode,
        ty: EventType,
    ) {
        self.heap.lock().unwrap().push(QueuedEvent {
            name,
            component,
            ty,
            requeue_mode,
            time: Reverse(time),
        });
    }

    #[inline]
    pub fn consume(&self, upto: Period) {
        let mut heap_guard = self.heap.lock().unwrap();

        while let Some(sync_point) = heap_guard.peek() {
            if upto < sync_point.time.0 {
                break;
            }
            let event = heap_guard.pop().unwrap();

            match event.requeue_mode {
                EventRequeueMode::Once => {}
                EventRequeueMode::Repeating { frequency } => {
                    let ty = match &event.ty {
                        EventType::SyncPoint => EventType::SyncPoint,
                        EventType::Input { id, state } => EventType::Input {
                            id: *id,
                            state: *state,
                        },
                        EventType::Custom { .. } => {
                            unreachable!("Cannot repeat custom events");
                        }
                    };

                    let time = event.time.0 + frequency.recip();

                    heap_guard.push(QueuedEvent {
                        component: event.component.clone(),
                        ty,
                        requeue_mode: event.requeue_mode,
                        time: Reverse(time),
                        name: event.name.clone(),
                    });
                }
            }
            drop(heap_guard);

            event.component.interact_mut(event.time.0, |component| {
                component.handle_event(&event.name, event.ty);
            });

            heap_guard = self.heap.lock().unwrap();
        }
    }

    #[inline]
    pub fn next_event(&self) -> Option<Period> {
        let queue_guard = self.heap.lock().unwrap();

        if let Some(next_event) = queue_guard.peek() {
            return Some(next_event.time.0);
        }

        None
    }
}

#[derive(Debug)]
struct QueuedEvent {
    component: ComponentHandle,
    ty: EventType,
    requeue_mode: EventRequeueMode,
    time: Reverse<Period>,
    name: Cow<'static, str>,
}

impl PartialEq for QueuedEvent {
    fn eq(&self, other: &Self) -> bool {
        self.time.eq(&other.time)
    }
}

impl Eq for QueuedEvent {}

impl PartialOrd for QueuedEvent {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for QueuedEvent {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.time.cmp(&other.time)
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum EventRequeueMode {
    Once,
    Repeating { frequency: Frequency },
}

#[derive(Debug, Default)]
pub(crate) struct PreemptionSignal(AtomicBool);

impl PreemptionSignal {
    pub(crate) fn event_occured(&self) {
        self.0.store(true, Ordering::Release);
    }

    #[inline]
    pub(crate) fn needs_preemption(&self) -> bool {
        if !self.0.load(Ordering::Acquire) {
            return false;
        }

        self.0.swap(false, Ordering::AcqRel)
    }
}
