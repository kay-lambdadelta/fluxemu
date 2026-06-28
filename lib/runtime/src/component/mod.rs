use std::{any::Any, fmt::Debug, ops::RangeInclusive};

use fluxemu_input::{InputId, InputState};
use fluxemu_range::ContiguousRange;
use nalgebra::SVector;
use ringbuffer::AllocRingBuffer;

use crate::{
    event::Event,
    memory::{Address, AddressSpaceId, MemoryError, MemoryErrorType},
    scheduler::{Period, SynchronizationContext},
};

/// Component config (factory) related items
pub mod config;
mod registry;

pub use registry::*;

/// Basic supertrait for all components
///
/// NONE of these methods should be directly called by other components, they are for runtime use only.
/// They often have invariants only the runtime knows how to properly enforce
#[allow(unused)]
pub trait Component: Send + Sync + Debug + Any {
    /// Event type component accepts
    ///
    /// Use `()` if you don't care about events.
    /// You may still receive dummy events being used as a synchronization barrier however
    ///
    /// FIXME: When rust gets default associated types, this should be `()`
    type Event: Event
    where
        Self: Sized;

    /// Read memory at the specified address given the address space id
    ///
    /// The avoid side effects flag should be respected, state changes should not occur as a result of the operation if it is true
    ///
    /// The default implementation of this simply denies
    fn memory_read(
        &mut self,
        address: Address,
        address_space: AddressSpaceId,
        avoid_side_effects: bool,
        buffer: &mut [u8],
    ) -> Result<(), MemoryError> {
        Err(denied_range(address, buffer.len()))
    }

    /// Write memory to the specified address given the address space id
    ///
    /// The default implementation of this simply denies
    fn memory_write(
        &mut self,
        address: Address,
        address_space: AddressSpaceId,
        buffer: &[u8],
    ) -> Result<(), MemoryError> {
        Err(denied_range(address, buffer.len()))
    }

    /// Inform the component it should treat `base` as the new base address for its operations
    fn memory_rebase(&mut self, base: Address) {
        unreachable!("This component does not support rebasing");
    }

    /// Returns the audio channel with the given name, based upon what this component registered
    fn get_audio_channel(&mut self, name: &str) -> SampleSource<'_> {
        unreachable!()
    }

    /// Returns the framebuffer with the given name, based upon what this component registered
    ///
    /// This should be downcasted to [`GraphicsApi::Framebuffer`](fluxemu_graphics::api::GraphicsApi::Framebuffer)
    fn get_framebuffer(&mut self, name: &str) -> &dyn Any {
        unreachable!()
    }

    /// Synchronize using the utilties given by [`SynchronizationContext`]
    fn synchronize(&mut self, context: SynchronizationContext) {}

    /// Tell the scheduler that work needs to be done to close this delta
    ///
    /// It is logically hazardous to do any runtime interaction within this function
    fn needs_work(&self, current_timestamp: &Period, delta: &Period) -> bool {
        false
    }

    /// Handle an event targeted towards this component
    fn handle_event(&mut self, event: Box<dyn Event>) {}

    /// Inform the component of an input event from the runtime
    ///
    /// This will as an invariant, only pass in inputs the component registered as supporting
    fn handle_input(&mut self, destination: &str, id: InputId, state: InputState) {}
}

/// A source of audio samples for the runtime
pub struct SampleSource<'a> {
    /// A ring buffer of audio samples
    pub audio_ring: &'a mut AllocRingBuffer<SVector<f32, 1>>,
    /// The sample rate in which to properly interpret `audio_ring`
    pub sample_rate: f32,
}

#[inline]
fn denied_range(address: Address, len: usize) -> MemoryError {
    MemoryError(
        std::iter::once((
            RangeInclusive::from_start_and_length(address, len),
            MemoryErrorType::Denied,
        ))
        .collect(),
    )
}

/// A nonstable ID to refer to a component.
///
/// Use a path if stability is a concern, but use this if absolute speed is more of a concern
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ComponentId(pub(crate) u16);
