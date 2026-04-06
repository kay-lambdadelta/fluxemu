use dyn_clone::DynClone;
use serde::{Deserialize, Serialize};

use crate::{
    ComponentPath, RuntimeApi,
    component::{Component, handle::ComponentHandle},
    scheduler::{Frequency, Period},
};
use std::{
    any::Any,
    cmp::Reverse,
    collections::BinaryHeap,
    fmt::Debug,
    sync::{
        Mutex,
        atomic::{AtomicBool, Ordering},
    },
};

pub trait Event: Any + DynClone + Send + Debug + 'static {}
impl<T: Any + DynClone + Send + Debug + 'static> Event for T {}

#[derive(Debug, Default)]
pub(crate) struct EventManager {
    heap: Mutex<BinaryHeap<QueuedEvent>>,
}

impl EventManager {
    #[inline]
    pub fn schedule(
        &self,
        time: Period,
        path: ComponentPath,
        mode: EventMode,
        data: Box<dyn Event>,
    ) {
        self.heap.lock().unwrap().push(QueuedEvent {
            path,
            data,
            mode,
            time: Reverse(time),
        });
    }

    #[inline]
    pub fn consume(
        &self,
        active_component: &mut ComponentHandle,
        runtime: &RuntimeApi,
        upto: Period,
    ) {
        let mut heap_guard = self.heap.lock().unwrap();

        while let Some(event) = heap_guard.peek() {
            if upto < event.time.0 {
                break;
            }
            let event = heap_guard.pop().unwrap();

            match event.mode {
                EventMode::Once => {}
                EventMode::Repeating { frequency } => {
                    let time = event.time.0 + frequency.recip();

                    heap_guard.push(QueuedEvent {
                        path: event.path.clone(),
                        mode: event.mode,
                        time: Reverse(time),
                        data: dyn_clone::clone_box(event.data.as_ref()),
                    });
                }
            }

            drop(heap_guard);

            if active_component.path() == &event.path {
                active_component.interact_mut(runtime, event.time.0, |component| {
                    component.handle_event(event.data);
                })
            } else {
                runtime
                    .registry()
                    .interact_dyn_mut(&event.path, event.time.0, |component| {
                        component.handle_event(event.data);
                    });
            }

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

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum EventMode {
    Once,
    Repeating { frequency: Frequency },
}

#[inline]
pub fn downcast_event<C: Component>(event: Box<dyn Event>) -> C::Event {
    *(event as Box<dyn Any>)
        .downcast()
        .expect("invalid type sent as event")
}

#[derive(Debug)]
struct QueuedEvent {
    path: ComponentPath,
    time: Reverse<Period>,
    mode: EventMode,
    data: Box<dyn Event>,
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
