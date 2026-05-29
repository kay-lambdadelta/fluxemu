use std::range::RangeInclusive;

use fluxemu_range::{ContiguousRange, RangeIntersection};
use num::traits::{FromBytes, ops::bytes::NumBytes};

use super::AddressSpace;
use crate::{
    component::{Component, ComponentId, ComponentRegistry},
    memory::{
        Address, AddressSpaceId, MemoryError, PAGE_SIZE, PageEntry, PageTarget, component::Memory,
        form_error,
    },
    scheduler::Period,
};

impl<'a> AddressSpace<'a> {
    #[inline]
    pub(super) fn read_internal<B: NumBytes + ?Sized>(
        &mut self,
        mut address: Address,
        timestamp: Period,
        avoid_side_effects: bool,
        buffer: &mut B,
    ) -> Result<(), MemoryError> {
        let members = self.data.get_members(&self.guard);

        address &= self.data.width_mask;
        let mut remaining_buffer = buffer.as_mut();

        while !remaining_buffer.is_empty() {
            let mut handled = false;

            let max_to_width_boundary = self.data.width_mask - address;
            let chunk_len = if remaining_buffer.len() - 1 <= max_to_width_boundary {
                remaining_buffer.len()
            } else {
                max_to_width_boundary + 1
            };

            let (chunk_buffer, next_buffer) = remaining_buffer.split_at_mut(chunk_len);
            remaining_buffer = next_buffer;

            let access_range = RangeInclusive::from_start_and_length(address, chunk_len);

            let start_page = access_range.start / PAGE_SIZE;
            let end_page = access_range.last / PAGE_SIZE;

            // SAFETY: The start and end pages are bounded by the width mask, they fall into the table constructed by `commit`
            let page_slice = unsafe {
                members
                    .read
                    .computed_table
                    .get_unchecked(start_page..=end_page)
            };

            for PageEntry {
                range: entry_assigned_range,
                target,
            } in page_slice.iter().flatten()
            {
                if entry_assigned_range.last < access_range.start {
                    continue;
                }

                if entry_assigned_range.start > access_range.last {
                    break;
                }

                handled = true;

                let entry_access_range = entry_assigned_range.intersection(&access_range);
                let offset = entry_access_range.start - entry_assigned_range.start;

                let buffer_range = (entry_access_range.start - access_range.start)
                    ..=(entry_access_range.last - access_range.start);

                let adjusted_buffer = &mut chunk_buffer[buffer_range];

                match target {
                    PageTarget::Component {
                        destination_start,
                        component_id,
                        is_standard_memory,
                    } => {
                        let destination = destination_start + offset;

                        if *is_standard_memory {
                            // We perform a manual devirtualization here because it can often prevent a indirect call
                            // and speaking to the internals to the standard memory component specifically, a memcpy call

                            self.registry
                                .interact_dyn(
                                    *component_id,
                                    timestamp,
                                    #[inline]
                                    |component| {
                                        // SAFETY: In `commit` is_standard_memory is set based upon the typeid of the component
                                        //
                                        // This is basically doing a stable `downcast_unchecked`
                                        let component = unsafe {
                                            &mut *(std::ptr::from_mut(component) as *mut Memory)
                                        };

                                        component.memory_read(
                                            destination,
                                            self.data.id,
                                            avoid_side_effects,
                                            adjusted_buffer,
                                        )
                                    },
                                )
                                .unwrap()?;
                        } else {
                            virtual_memory_read(
                                *component_id,
                                timestamp,
                                self.registry,
                                destination,
                                self.data.id,
                                avoid_side_effects,
                                adjusted_buffer,
                            )?;
                        }
                    }
                    PageTarget::Memory(bytes) => {
                        let memory_range =
                            RangeInclusive::from_start_and_length(offset, adjusted_buffer.len());

                        // SAFETY: `commit` ensures memory byte entries are the same size as the range they are assigned to
                        let bytes = unsafe { bytes.get_unchecked(memory_range) };

                        adjusted_buffer.copy_from_slice(bytes);
                    }
                }

                if entry_access_range.last == access_range.last {
                    break;
                }
            }

            if !handled {
                return Err(form_error(access_range.into()));
            }

            address = (address + chunk_len) & self.data.width_mask;
        }

        Ok(())
    }

    /// Read a buffer from an address
    #[inline]
    pub fn read(
        &mut self,
        address: Address,
        current_timestamp: Period,
        buffer: &mut [u8],
    ) -> Result<(), MemoryError> {
        self.read_internal(address, current_timestamp, false, buffer)
    }

    /// Read a buffer from an address, informing the component that this should not induce state change as a direct result of a read.
    /// Synchronization will still occur.
    #[inline]
    pub fn read_pure(
        &mut self,
        address: Address,
        current_timestamp: Period,
        buffer: &mut [u8],
    ) -> Result<(), MemoryError> {
        self.read_internal(address, current_timestamp, true, buffer)
    }

    /// Read a little endian value from an address
    #[inline]
    pub fn read_le_value<T: FromBytes>(
        &mut self,
        address: Address,
        current_timestamp: Period,
    ) -> Result<T, MemoryError>
    where
        T::Bytes: Default,
    {
        let mut buffer = T::Bytes::default();
        self.read_internal(address, current_timestamp, false, &mut buffer)?;
        Ok(T::from_le_bytes(&buffer))
    }

    /// Read a little endian value from an address, informing the component that this should not induce state change as a direct result of a read.
    /// Synchronization will still occur.
    #[inline]
    pub fn read_le_value_pure<T: FromBytes>(
        &mut self,
        address: Address,
        current_timestamp: Period,
    ) -> Result<T, MemoryError>
    where
        T::Bytes: Default,
    {
        let mut buffer = T::Bytes::default();
        self.read_internal(address, current_timestamp, true, &mut buffer)?;
        Ok(T::from_le_bytes(&buffer))
    }

    /// Read a big endian value from an address
    #[inline]
    pub fn read_be_value<T: FromBytes>(
        &mut self,
        address: Address,
        current_timestamp: Period,
    ) -> Result<T, MemoryError>
    where
        T::Bytes: Default,
    {
        let mut buffer = T::Bytes::default();
        self.read_internal(address, current_timestamp, false, &mut buffer)?;
        Ok(T::from_be_bytes(&buffer))
    }

    /// Read a big endian value from an address, informing the component that this should not induce state change as a direct result of a read.
    /// Synchronization will still occur.
    #[inline]
    pub fn read_be_value_pure<T: FromBytes>(
        &mut self,
        address: Address,
        current_timestamp: Period,
    ) -> Result<T, MemoryError>
    where
        T::Bytes: Default,
    {
        let mut buffer = T::Bytes::default();
        self.read_internal(address, current_timestamp, true, &mut buffer)?;
        Ok(T::from_be_bytes(&buffer))
    }
}

#[cold]
fn virtual_memory_read(
    component_id: ComponentId,
    timestamp: Period,
    registry: ComponentRegistry<'_>,
    destination: usize,
    address_space_id: AddressSpaceId,
    avoid_side_effects: bool,
    buffer: &mut [u8],
) -> Result<(), MemoryError> {
    registry
        .interact_dyn(
            component_id,
            timestamp,
            #[inline]
            |component| {
                component.memory_read(destination, address_space_id, avoid_side_effects, buffer)
            },
        )
        .unwrap()
}
