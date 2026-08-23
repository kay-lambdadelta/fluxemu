use std::{any::Any, fmt::Debug};

use dyn_clone::DynClone;
use serde::{Deserialize, Serialize};

use crate::{component::Component, scheduler::Frequency};

/// Supertrait aggregate type for events
pub trait Event: Any + DynClone + Send + Debug + 'static {}
impl<T: Any + DynClone + Send + Debug + 'static> Event for T {}

/// Mode of an event
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum EventMode {
    /// This event will be run once and then discarded
    Once,
    /// This event will be rescheduled every execution
    Repeating {
        /// The frequency at which this event will be rescheduled
        frequency: Frequency,
    },
}

/// Downcast the event down to the type that it should be
#[inline]
pub fn downcast_event<C: Component>(event: Box<dyn Event>) -> C::Event {
    *(event as Box<dyn Any>)
        .downcast()
        .expect("invalid type sent as event")
}
