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
    component::{ComponentHandle, Event},
    scheduler::{Frequency, Period},
};

#[derive(Debug, Default)]
pub struct SyncPointManager {
    queue: Mutex<BinaryHeap<SyncPointQueueWrapper>>,
}

impl SyncPointManager {
    pub(crate) fn queue(
        &self,
        component: ComponentHandle,
        time: Period,
        ty: EventType,
        name: Cow<'static, str>,
    ) {
        self.queue
            .lock()
            .unwrap()
            .push(SyncPointQueueWrapper(SyncPoint {
                component,
                ty,
                time: Reverse(time),
                name,
            }));
    }

    pub fn consume(&self, upto: Period) {
        let mut queue_guard = self.queue.lock().unwrap();

        while let Some(sync_point) = queue_guard.peek() {
            if upto < sync_point.0.time.0 {
                break;
            }
            let SyncPointQueueWrapper(mut sync_point) = queue_guard.pop().unwrap();

            drop(queue_guard);

            match sync_point.ty {
                EventType::Once => {
                    sync_point
                        .component
                        .interact_mut(sync_point.time.0, |component| {
                            component.handle_event(Event::SyncPoint {
                                name: sync_point.name.as_ref(),
                            });
                        });
                    queue_guard = self.queue.lock().unwrap();
                }
                EventType::Repeating { frequency } => {
                    sync_point
                        .component
                        .interact_mut(sync_point.time.0, |component| {
                            component.handle_event(Event::SyncPoint {
                                name: sync_point.name.as_ref(),
                            });
                        });
                    queue_guard = self.queue.lock().unwrap();

                    sync_point.time.0 += frequency.recip();

                    queue_guard.push(SyncPointQueueWrapper(sync_point));
                }
            }
        }
    }

    #[inline]
    pub fn next_event(&self) -> Option<Period> {
        let queue_guard = self.queue.lock().unwrap();

        if let Some(next_event) = queue_guard.peek() {
            return Some(next_event.0.time.0);
        }

        None
    }
}

#[derive(Debug)]
struct SyncPoint {
    component: ComponentHandle,
    ty: EventType,
    time: Reverse<Period>,
    name: Cow<'static, str>,
}

#[derive(Debug)]
/// Wrapper so the min heap works as anticipated
struct SyncPointQueueWrapper(SyncPoint);

impl PartialEq for SyncPointQueueWrapper {
    fn eq(&self, other: &Self) -> bool {
        self.0.time.eq(&other.0.time)
    }
}

impl Eq for SyncPointQueueWrapper {}

impl PartialOrd for SyncPointQueueWrapper {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for SyncPointQueueWrapper {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.0.time.cmp(&other.0.time)
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub(crate) enum EventType {
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
