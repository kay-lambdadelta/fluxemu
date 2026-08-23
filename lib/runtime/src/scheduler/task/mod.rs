mod frequency;

use std::any::Any;
use std::fmt::Debug;

use crate::{
    component::Component,
    scheduler::{Context, Period},
};

pub use frequency::FrequencyBased;

pub trait Task: Debug + Send + Sync + 'static {
    type Component: Component;

    /// Run and simulate time until the context enforces a yield, then return when this system should be run next
    fn run(&mut self, component: &mut Self::Component, context: &mut Context) -> Period;
}

/// Convenience trait so we can store tasks inside the component registry
pub(crate) trait DynTask: Debug + Send + Sync + 'static {
    fn run(&mut self, component: &mut dyn Component, context: &mut Context) -> Period;
}

impl<S: Task> DynTask for S {
    fn run(&mut self, component: &mut dyn Component, context: &mut Context) -> Period {
        let component = (component as &mut dyn Any).downcast_mut().unwrap();

        self.run(component, context)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Mode {
    /// This task will be run strictly on the timestamp, using the global priority queue
    Always,
    /// Task execution will be delayed until an interaction occurs on the associated component
    OnDemand,
}
