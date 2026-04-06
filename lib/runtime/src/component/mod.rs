use std::{
    any::Any,
    fmt::Debug,
    io::{Read, Write},
    ops::RangeInclusive,
    sync::atomic::{AtomicU32, Ordering},
};

use fluxemu_input::{InputId, InputState};
use fluxemu_range::ContiguousRange;
use nalgebra::SVector;
use ringbuffer::AllocRingBuffer;

use crate::{
    event::Event,
    memory::{Address, AddressSpaceId, MemoryError, MemoryErrorType},
    scheduler::{Period, SynchronizationContext},
};

pub mod config;
pub(crate) mod handle;
mod registry;

pub(crate) use registry::ComponentRegistryData;
pub use registry::*;

#[allow(unused)]
/// Basic supertrait for all components
pub trait Component: Send + Sync + Debug + Any {
    /// Event type component accepts
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
        version: ComponentVersion,
        reader: &mut dyn Read,
    ) -> Result<(), Box<dyn std::error::Error>> {
        Ok(())
    }

    /// Read memory at the specified address in the specified address space to fill the buffer
    ///
    /// Nothing should never explicitly call this, instead going through [crate::memory::AddressSpace]
    fn memory_read(
        &self,
        address: Address,
        address_space: AddressSpaceId,
        avoid_side_effects: bool,
        buffer: &mut [u8],
    ) -> Result<(), MemoryError> {
        Err(denied_range(address, buffer.len()))
    }

    /// Writes memory at the specified address in the specified address space
    ///
    /// Nothing should never explicitly call this, instead going through [crate::memory::AddressSpace]
    fn memory_write(
        &mut self,
        address: Address,
        address_space: AddressSpaceId,
        buffer: &[u8],
    ) -> Result<(), MemoryError> {
        Err(denied_range(address, buffer.len()))
    }

    /// Give the runtime the audio sample ring buffer
    fn get_audio_channel(&mut self, name: &str) -> SampleSource<'_> {
        unreachable!()
    }

    /// Synchronize in a loop until the iterator ends
    fn synchronize(&mut self, context: SynchronizationContext) {}

    /// Tell the scheduler that work needs to be done to close this delta
    fn needs_work(&self, delta: Period) -> bool {
        false
    }

    /// Handle an event targeted towards this component
    fn handle_event(&mut self, event: Box<dyn Event>) {}

    /// Handle some input targeted at destination
    fn handle_input(&mut self, destination: &str, id: InputId, state: InputState) {}
}

/// Version that components use
pub type ComponentVersion = u32;

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

impl ComponentId {
    pub(crate) fn new() -> Self {
        static ID_COUNTER: AtomicU32 = AtomicU32::new(0);
        let id = ID_COUNTER.fetch_add(1, Ordering::Relaxed);

        if id == u32::MAX {
            unreachable!("Too many components");
        }

        ComponentId(id)
    }
}
