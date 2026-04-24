use std::{
    any::Any,
    fmt::Debug,
    io::{Read, Write},
    ops::RangeInclusive,
};

use fluxemu_input::{InputId, InputState};
use fluxemu_range::ContiguousRange;
use nalgebra::SVector;
use ringbuffer::AllocRingBuffer;

use crate::{
    event::Event,
    memory::{Address, AddressSpaceId, MemoryError, MemoryErrorType},
    persistence::PersistanceFormatVersion,
    scheduler::{Period, SynchronizationContext},
};

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
    type Event: Event
    where
        Self: Sized;

    /// Write a save representative of the current state of the save relevant aspects of the component
    fn store_save(&self, writer: &mut dyn Write) -> Result<(), Box<dyn std::error::Error>> {
        Ok(())
    }

    /// Write a snapshot representative of the current state of the component
    fn store_snapshot(&self, writer: &mut dyn Write) -> Result<(), Box<dyn std::error::Error>> {
        Ok(())
    }

    /// Read a snapshot and restore the state given within it
    fn load_snapshot(
        &mut self,
        version: PersistanceFormatVersion,
        reader: &mut dyn Read,
    ) -> Result<(), Box<dyn std::error::Error>> {
        Ok(())
    }

    /// Read memory at the specified address given the address space id
    ///
    /// The avoid side effects flag should be respected, state changes should not occur as a result of the read
    /// if it is true
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

    fn memory_rebase(&mut self, base: Address) {
        unreachable!("This component does not support rebasing");
    }

    /// Give the runtime the audio sample ring buffer
    fn get_audio_channel(&mut self, name: &str) -> SampleSource<'_> {
        unreachable!()
    }

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

    /// Handle some input targeted at destination
    fn handle_input(&mut self, destination: &str, id: InputId, state: InputState) {}
}

pub struct SampleSource<'a> {
    pub source: &'a mut AllocRingBuffer<SVector<f32, 1>>,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ComponentId(pub(crate) u32);
