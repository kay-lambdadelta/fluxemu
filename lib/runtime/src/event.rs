use std::{
    any::Any,
    borrow::Cow,
    cmp::Reverse,
    collections::BinaryHeap,
    fmt::Debug,
    sync::{
        Mutex,
        atomic::{AtomicBool, Ordering},
    },
};

use fluxemu_input::{InputId, InputState};
use serde::{Deserialize, Serialize};

use crate::{
    component::ComponentHandle,
    scheduler::{Frequency, Period},
};

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum EventRequeueMode {
    Once,
    Repeating { frequency: Frequency },
}

#[derive(Debug)]
pub enum EventType {
    // Synchronization point, intended to force a component to be updated at a time
    SyncPoint,
    // Input event, for listening inputs
    Input { id: InputId, state: InputState },
    // Custom event, for sending custom data to components
    Custom { data: Box<dyn EventImpl> },
}

impl EventType {
    pub fn sync_point() -> Self {
        Self::SyncPoint
    }

    pub fn input(id: InputId, state: InputState) -> Self {
        Self::Input { id, state }
    }

    pub fn custom(data: impl EventImpl) -> Self {
        Self::Custom {
            data: Box::new(data),
        }
    }
}

pub trait EventImpl: Any + Send + Debug {}
impl<T: Any + Send + Debug> EventImpl for T {}

#[derive(Debug, Default)]
pub(crate) struct EventManager {
    heap: Mutex<BinaryHeap<QueuedEvent>>,
}

impl EventManager {
    #[inline]
    pub fn queue(
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
pub(crate) struct PreemptionSignal(AtomicBool);

impl PreemptionSignal {
    pub(super) fn new() -> Self {
        Self(AtomicBool::new(false))
    }

    pub(crate) fn event_occurred(&self) {
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
