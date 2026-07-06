use std::{range::RangeInclusive, sync::Arc};

use fluxemu_range::{ContiguousRange, RangeIntersection};
use num::traits::{FromBytes, ToBytes, ops::bytes::NumBytes};

use super::AddressSpace;
use crate::{
    component::{ComponentId, ComponentRegistry},
    memory::{
        Address, AddressSpaceId, CHUNK_SIZE, MemoryError, MemoryErrorType, PageTableEntry,
        PageTableTarget,
    },
    scheduler::Period,
};

impl<'a> AddressSpace<'a> {
    #[inline]
    pub(super) fn read_internal<B: NumBytes + ?Sized, const AVOID_SIDE_EFFECTS: bool>(
        &mut self,
        address: Address,
        timestamp: &Period,
        buffer: &mut B,
    ) -> Result<(), MemoryError> {
        let members = self.data.get_members(&self.guard);

        for Chunk {
            access_range,
            page_table_slice,
            buffer: chunk_buffer,
        } in ChunkIter::new(
            address,
            self.data.width_mask,
            buffer.as_mut(),
            &members.read.0,
        ) {
            visit_page_entries(
                page_table_slice,
                access_range,
                chunk_buffer,
                #[inline]
                |target, offset, adjusted| {
                    match target {
                        PageTableTarget::ImmutableMemory(bytes) => {
                            let memory_range =
                                RangeInclusive::from_start_and_length(offset, adjusted.len());

                            // SAFETY: `commit` ensures memory byte entries are the same size as the range they are assigned to
                            let bytes = unsafe { bytes.get_unchecked(memory_range) };

                            adjusted.copy_from_slice(bytes);
                        }
                        PageTableTarget::Memory {
                            offset: destination_start,
                            id,
                        } => {
                            let destination = destination_start + offset;

                            self.memory_registry.read(*id, destination, adjusted);
                        }
                        PageTableTarget::Component {
                            offset: destination_start,
                            id,
                        } => {
                            let destination = destination_start + offset;

                            virtual_memory_read::<AVOID_SIDE_EFFECTS>(
                                *id,
                                timestamp,
                                destination,
                                self.data.id,
                                adjusted,
                                &mut self.component_registry,
                            )?;
                        }
                    }
                    Ok(())
                },
            )?;
        }

        Ok(())
    }

    /// Read a buffer from an address
    ///
    /// If the target is a component, the component will be advanced to the timestamp before the operation
    ///
    /// # Error Behavior
    ///
    /// It is completely unspecified what the buffer will contain if an error occurs during an read operation
    #[inline]
    pub fn read<B: NumBytes + ?Sized>(
        &mut self,
        address: Address,
        current_timestamp: &Period,
        buffer: &mut B,
    ) -> Result<(), MemoryError> {
        self.read_internal::<_, false>(address, current_timestamp, buffer)
    }

    /// Read a buffer from an address
    ///
    /// If the target is a component, the component will be advanced to the timestamp before the operation
    /// Additionally, the component will be informed it should not change state as a result of the operation (such as a flag clear)
    ///
    /// It is completely unspecified what the buffer will contain if an error occurs during an read operation
    #[inline]
    pub fn read_pure<B: NumBytes + ?Sized>(
        &mut self,
        address: Address,
        current_timestamp: &Period,
        buffer: &mut B,
    ) -> Result<(), MemoryError> {
        self.read_internal::<_, true>(address, current_timestamp, buffer)
    }

    /// Convenience method for reading a little endian value from an address
    ///
    /// Has the same behavior as [`read`](Self::read)
    #[inline]
    pub fn read_le_value<T: FromBytes>(
        &mut self,
        address: Address,
        current_timestamp: &Period,
    ) -> Result<T, MemoryError>
    where
        T::Bytes: Default,
    {
        let mut buffer = T::Bytes::default();
        self.read_internal::<_, false>(address, current_timestamp, &mut buffer)?;
        Ok(T::from_le_bytes(&buffer))
    }

    /// Convenience method for reading a little endian value from an address
    ///
    /// Has the same behavior as [`read_pure`](Self::read_pure)
    #[inline]
    pub fn read_le_value_pure<T: FromBytes>(
        &mut self,
        address: Address,
        current_timestamp: &Period,
    ) -> Result<T, MemoryError>
    where
        T::Bytes: Default,
    {
        let mut buffer = T::Bytes::default();
        self.read_internal::<_, true>(address, current_timestamp, &mut buffer)?;
        Ok(T::from_le_bytes(&buffer))
    }

    /// Convenience method for reading a big endian value from an address
    ///
    /// Has the same behavior as [`read`](Self::read)
    #[inline]
    pub fn read_be_value<T: FromBytes>(
        &mut self,
        address: Address,
        current_timestamp: &Period,
    ) -> Result<T, MemoryError>
    where
        T::Bytes: Default,
    {
        let mut buffer = T::Bytes::default();
        self.read_internal::<_, false>(address, current_timestamp, &mut buffer)?;
        Ok(T::from_be_bytes(&buffer))
    }

    /// Convenience method for reading a big endian value from an address
    ///
    /// Has the same behavior as [`read_pure`](Self::read_pure)
    #[inline]
    pub fn read_be_value_pure<T: FromBytes>(
        &mut self,
        address: Address,
        current_timestamp: &Period,
    ) -> Result<T, MemoryError>
    where
        T::Bytes: Default,
    {
        let mut buffer = T::Bytes::default();
        self.read_internal::<_, true>(address, current_timestamp, &mut buffer)?;
        Ok(T::from_be_bytes(&buffer))
    }

    #[inline]
    pub(super) fn write_internal<B: NumBytes + ?Sized>(
        &mut self,
        address: Address,
        timestamp: &Period,
        buffer: &B,
    ) -> Result<(), MemoryError> {
        let members = self.data.get_members(&self.guard);

        for Chunk {
            access_range,
            page_table_slice,
            buffer: chunk_buffer,
        } in ChunkIter::new(
            address,
            self.data.width_mask,
            buffer.as_ref(),
            &members.write.0,
        ) {
            visit_page_entries(
                page_table_slice,
                access_range,
                chunk_buffer,
                #[inline]
                |target, offset, adjusted| {
                    match target {
                        PageTableTarget::Memory {
                            offset: destination_start,
                            id,
                        } => {
                            let destination = destination_start + offset;

                            self.memory_registry.write(*id, destination, adjusted);
                        }
                        PageTableTarget::Component {
                            offset: destination_start,
                            id,
                        } => {
                            let destination = destination_start + offset;

                            virtual_memory_write(
                                *id,
                                timestamp,
                                destination,
                                self.data.id,
                                adjusted,
                                &mut self.component_registry,
                            )?;
                        }
                        PageTableTarget::ImmutableMemory(_) => unreachable!(),
                    }

                    Ok(())
                },
            )?;
        }

        Ok(())
    }

    /// Write a buffer to an address
    ///
    /// If the target is a component, the component will be advanced to the timestamp before the operation
    ///
    /// It is completely unspecified what parts of the buffer will be written if an error occurs midway through the operation
    #[inline]
    pub fn write<B: NumBytes + ?Sized>(
        &mut self,
        address: Address,
        current_timestamp: &Period,
        buffer: &B,
    ) -> Result<(), MemoryError> {
        self.write_internal(address, current_timestamp, buffer)
    }

    /// Convenience method for writing a little endian value to an address
    ///
    /// Has the same behavior as [`write`](Self::write)
    #[inline]
    pub fn write_le_value<T: ToBytes>(
        &mut self,
        address: Address,
        current_timestamp: &Period,
        value: T,
    ) -> Result<(), MemoryError> {
        self.write_internal(address, current_timestamp, &value.to_le_bytes())
    }

    /// Convenience method for writing a big endian value to an address
    ///
    /// Has the same behavior as [`write`](Self::write)
    #[inline]
    pub fn write_be_value<T: ToBytes>(
        &mut self,
        address: Address,
        current_timestamp: &Period,
        value: T,
    ) -> Result<(), MemoryError> {
        self.write_internal(address, current_timestamp, &value.to_be_bytes())
    }
}

trait SplitableBuffer: Sized {
    fn empty() -> Self;
    fn split(self, mid: usize) -> (Self, Self);
    fn len(&self) -> usize;
}

impl SplitableBuffer for &[u8] {
    #[inline]
    fn empty() -> Self {
        &[]
    }

    #[inline]
    fn split(self, mid: usize) -> (Self, Self) {
        self.split_at(mid)
    }

    #[inline]
    fn len(&self) -> usize {
        <[_]>::len(self)
    }
}

impl SplitableBuffer for &mut [u8] {
    #[inline]
    fn empty() -> Self {
        &mut []
    }

    #[inline]
    fn split(self, mid: usize) -> (Self, Self) {
        self.split_at_mut(mid)
    }

    #[inline]
    fn len(&self) -> usize {
        <[_]>::len(self)
    }
}

struct Chunk<'a, BUFFER> {
    access_range: RangeInclusive<usize>,
    buffer: BUFFER,
    page_table_slice: &'a [Arc<[PageTableEntry]>],
}

struct ChunkIter<'a, BUFFER> {
    address: Address,
    width_mask: usize,
    buffer: BUFFER,
    page_table: &'a [Arc<[PageTableEntry]>],
}

impl<'a, BUFFER: SplitableBuffer> ChunkIter<'a, BUFFER> {
    #[inline]
    fn new(
        address: Address,
        width_mask: usize,
        buffer: BUFFER,
        page_table: &'a [Arc<[PageTableEntry]>],
    ) -> Self {
        Self {
            address: address & width_mask,
            width_mask,
            buffer,
            page_table,
        }
    }
}

impl<'a, BUFFER: SplitableBuffer> Iterator for ChunkIter<'a, BUFFER> {
    type Item = Chunk<'a, BUFFER>;

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        if self.buffer.len() == 0 {
            return None;
        }

        let max_to_width_boundary = self.width_mask - self.address;
        let chunk_len = if self.buffer.len() - 1 <= max_to_width_boundary {
            self.buffer.len()
        } else {
            max_to_width_boundary + 1
        };

        let buffer = std::mem::replace(&mut self.buffer, SplitableBuffer::empty());
        let (chunk_buffer, next_buffer) = buffer.split(chunk_len);
        self.buffer = next_buffer;

        let access_range = RangeInclusive::from_start_and_length(self.address, chunk_len);
        let page_range = (access_range.start / CHUNK_SIZE)..=(access_range.last / CHUNK_SIZE);

        let chunk = Chunk {
            access_range,
            // SAFETY: The start and end pages are bounded by the width mask, they fall into the table constructed by `commit`
            page_table_slice: unsafe { self.page_table.get_unchecked(page_range) },
            buffer: chunk_buffer,
        };

        self.address = (self.address + chunk_len) & self.width_mask;

        Some(chunk)
    }
}

#[inline]
fn visit_page_entries<BUFFER: SplitableBuffer>(
    page_table_slice: &[Arc<[PageTableEntry]>],
    access_range: RangeInclusive<usize>,
    buffer: BUFFER,
    mut callback: impl FnMut(&PageTableTarget, usize, BUFFER) -> Result<(), MemoryError>,
) -> Result<(), MemoryError> {
    let initial_buffer_len = buffer.len();
    let mut remaining = buffer;

    for page in page_table_slice {
        for PageTableEntry {
            range: entry_assigned_range,
            target,
        } in page.iter().filter(|entry| {
            entry.range.last >= access_range.start && entry.range.start <= access_range.last
        }) {
            let entry_access_range = entry_assigned_range.intersection(&access_range);
            let buffer_range = (entry_access_range.start - access_range.start)
                ..=(entry_access_range.last - access_range.start);

            let consumed = initial_buffer_len - remaining.len();

            // Potentially skip the duplicate entry for the last entry we processed that is on the next page
            if *buffer_range.end() < consumed {
                continue;
            }

            let gap = buffer_range.start() - consumed;
            if gap > 0 {
                return Err(form_error(RangeInclusive::from_start_and_length(
                    access_range.start + consumed,
                    gap,
                )));
            }

            let offset = entry_access_range.start - entry_assigned_range.start;

            let (adjusted_buffer, rest) = remaining.split(buffer_range.len());
            remaining = rest;

            callback(target, offset, adjusted_buffer)?;

            if entry_access_range.last == access_range.last {
                break;
            }
        }
    }

    if remaining.len() > 0 {
        let consumed = initial_buffer_len - remaining.len();

        return Err(form_error(RangeInclusive::from_start_and_length(
            access_range.start + consumed,
            remaining.len(),
        )));
    }

    Ok(())
}

fn virtual_memory_read<const AVOID_SIDE_EFFECTS: bool>(
    id: ComponentId,
    timestamp: &Period,
    destination: usize,
    address_space_id: AddressSpaceId,
    buffer: &mut [u8],
    component_registry: &mut ComponentRegistry,
) -> Result<(), MemoryError> {
    component_registry
        .interact_dyn(
            id,
            timestamp,
            #[inline]
            |component| {
                component.memory_read(destination, address_space_id, AVOID_SIDE_EFFECTS, buffer)
            },
        )
        .unwrap()
}

fn virtual_memory_write(
    id: ComponentId,
    timestamp: &Period,
    destination: usize,
    address_space_id: AddressSpaceId,
    buffer: &[u8],
    component_registry: &mut ComponentRegistry,
) -> Result<(), MemoryError> {
    component_registry
        .interact_dyn(
            id,
            timestamp,
            #[inline]
            |component| component.memory_write(destination, address_space_id, buffer),
        )
        .unwrap()
}

#[cold]
fn form_error(access_range: RangeInclusive<usize>) -> MemoryError {
    MemoryError(std::iter::once((access_range.into(), MemoryErrorType::OutOfBus)).collect())
}
