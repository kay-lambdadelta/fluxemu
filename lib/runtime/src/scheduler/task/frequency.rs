use std::{fmt::Debug, marker::PhantomData};

use crate::{
    RuntimeHandle,
    component::Component,
    scheduler::{Context, Frequency, Period, QuantaAllocator, task::Task},
};

/// System that wraps a closure to conveniently provide frequency based ticking with less boilerplate
pub struct FrequencyBased<C, MC> {
    period: Period,
    callback: MC,
    _phantom: PhantomData<fn() -> C>,
}

impl<C, MC> Debug for FrequencyBased<C, MC> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FrequencyBased")
            .field("period", &self.period)
            .finish()
    }
}

impl<
    C: Component,
    MC: FnMut(&mut C, &RuntimeHandle, QuantaAllocator<'_, '_>) + Send + Sync + 'static,
> FrequencyBased<C, MC>
{
    pub fn new(frequency: Frequency, callback: MC) -> Self {
        assert_ne!(frequency, Period::ZERO, "Frequency must not be zero");

        Self {
            period: frequency.recip(),
            callback,
            _phantom: PhantomData,
        }
    }
}

impl<
    C: Component,
    MC: FnMut(&mut C, &RuntimeHandle, QuantaAllocator<'_, '_>) + Send + Sync + 'static,
> Task for FrequencyBased<C, MC>
{
    type Component = C;

    #[inline]
    fn run(&mut self, component: &mut Self::Component, context: &mut Context) {
        let runtime_handle = context.runtime();
        let quanta_allocator = context.quanta_allocator(self.period);

        (self.callback)(component, runtime_handle, quanta_allocator);
    }
}
