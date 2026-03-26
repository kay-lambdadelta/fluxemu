use std::{
    any::Any,
    borrow::Cow,
    collections::HashMap,
    error::Error,
    fmt::Debug,
    io::{Read, Write},
    ops::RangeInclusive,
};

use fluxemu_input::{InputId, InputState};
use fluxemu_range::ContiguousRange;
pub use handle::*;
use nalgebra::SVector;
use ringbuffer::AllocRingBuffer;

use crate::{
    graphics::GraphicsApi,
    machine::builder::ComponentBuilder,
    memory::{Address, AddressSpaceId, MemoryError, MemoryErrorType},
    platform::Platform,
    scheduler::{Period, SynchronizationContext},
};

mod handle;

#[allow(unused)]
/// Basic supertrait for all components
pub trait Component: Send + Sync + Debug + Any {
    /// Write the save
    fn store_save(&self, writer: &mut dyn Write) -> Result<(), Box<dyn std::error::Error>> {
        Ok(())
    }

    /// Store the snapshot
    fn store_snapshot(&self, writer: &mut dyn Write) -> Result<(), Box<dyn std::error::Error>> {
        Ok(())
    }

    /// Read the snapshot
    fn load_snapshot(
        &mut self,
        version: ComponentVersion,
        reader: &mut dyn Read,
    ) -> Result<(), Box<dyn std::error::Error>> {
        Ok(())
    }

    /// Reads memory at the specified address in the specified address space to fill the buffer
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

    /// Synchronize until the time tracker indicates that no more time can be consumed
    fn synchronize(&mut self, context: SynchronizationContext) {}
    /// Given a delta between this components time and real time, is the component as much as it can be
    fn needs_work(&self, delta: Period) -> bool {
        false
    }

    fn handle_event(&mut self, name: &str, event: EventType) {}
}

#[allow(unused)]
/// Factory config to construct a component
pub trait ComponentConfig<P: Platform>: Debug + Sized + Sync + Send {
    /// The component that this config will create
    type Component: Component;

    /// Make a new component from the config
    fn build_component(
        self,
        component_builder: ComponentBuilder<P, Self::Component>,
    ) -> Result<Self::Component, Box<dyn Error>>;

    /// Do setup for subsystems that cannot be initalized during [`Self::build_component`]
    fn late_initialize(
        component: &mut Self::Component,
        data: &LateContext<P>,
    ) -> LateInitializedData<P> {
        Default::default()
    }
}

/// Data that the runtime will provide at the end of the initialization sequence
pub struct LateContext<P: Platform> {
    pub graphics_initialization_data: <P::GraphicsApi as GraphicsApi>::InitializationData,
}

pub struct LateInitializedData<P: Platform> {
    pub framebuffers: HashMap<Cow<'static, str>, <P::GraphicsApi as GraphicsApi>::Texture>,
}

impl<P: Platform> Default for LateInitializedData<P> {
    fn default() -> Self {
        Self {
            framebuffers: HashMap::default(),
        }
    }
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
