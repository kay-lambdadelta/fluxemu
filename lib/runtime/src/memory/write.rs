use std::ops::RangeInclusive;

use fluxemu_range::{ContiguousRange, RangeIntersection};
use num::traits::{ToBytes, ops::bytes::NumBytes};

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
    pub(super) fn write_internal<B: NumBytes + ?Sized>(
        &mut self,
        mut address: Address,
        timestamp: Period,
        buffer: &B,
    ) -> Result<(), MemoryError> {
        let members = self.data.get_members(&self.guard);

        address &= self.data.width_mask;
        let mut remaining_buffer = buffer.as_ref();

        while !remaining_buffer.is_empty() {
            let mut handled = false;

            let max_to_width_boundary = self.data.width_mask - address;
            let chunk_len = if remaining_buffer.len() - 1 <= max_to_width_boundary {
                remaining_buffer.len()
            } else {
                max_to_width_boundary + 1
            };

            let (chunk_buffer, next_buffer) = remaining_buffer.split_at(chunk_len);
            remaining_buffer = next_buffer;

            let access_range = RangeInclusive::from_start_and_length(address, chunk_len);

            let start_page = access_range.start() / PAGE_SIZE;
            let end_page = access_range.end() / PAGE_SIZE;

            // SAFETY: The start and end pages are bounded by the width mask, they fall into the table constructed by `commit`
            let page_slice = unsafe {
                members
                    .write
                    .computed_table
                    .get_unchecked(start_page..=end_page)
            };

            for PageEntry {
                range: entry_assigned_range,
                target,
            } in page_slice.iter().flatten()
            {
                if *entry_assigned_range.end() < *access_range.start() {
                    continue;
                }

                if *entry_assigned_range.start() > *access_range.end() {
                    break;
                }

                handled = true;

                let entry_access_range = entry_assigned_range.intersection(&access_range);
                let offset = entry_access_range.start() - entry_assigned_range.start();

                let buffer_range = (entry_access_range.start() - access_range.start())
                    ..=(entry_access_range.end() - access_range.start());

                let adjusted_buffer = &chunk_buffer[buffer_range];

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

                                        component.memory_write(
                                            destination,
                                            self.data.id,
                                            adjusted_buffer,
                                        )
                                    },
                                )
                                .unwrap()?;
                        } else {
                            virtual_memory_write(
                                *component_id,
                                timestamp,
                                self.registry,
                                destination,
                                self.data.id,
                                adjusted_buffer,
                            )?;
                        }
                    }
                    PageTarget::Memory(_) => {
                        unreachable!()
                    }
                }

                if entry_access_range.end() == access_range.end() {
                    break;
                }
            }

            if !handled {
                return Err(form_error(access_range));
            }

            address = (address + chunk_len) & self.data.width_mask;
        }

        Ok(())
    }

    /// Write a buffer to an address
    #[inline]
    pub fn write(
        &mut self,
        address: Address,
        current_timestamp: Period,
        buffer: &[u8],
    ) -> Result<(), MemoryError> {
        self.write_internal(address, current_timestamp, buffer)
    }

    /// Write a little endian value to an address
    ///
    /// This is generally faster than [Self::write], especially for single byte operations
    #[inline]
    pub fn write_le_value<T: ToBytes>(
        &mut self,
        address: Address,
        current_timestamp: Period,
        value: T,
    ) -> Result<(), MemoryError> {
        self.write_internal(address, current_timestamp, &value.to_le_bytes())
    }

    /// Write a big endian value to an address
    ///
    /// This is generally faster than [Self::write], especially for single byte operations
    #[inline]
    pub fn write_be_value<T: ToBytes>(
        &mut self,
        address: Address,
        current_timestamp: Period,
        value: T,
    ) -> Result<(), MemoryError> {
        self.write_internal(address, current_timestamp, &value.to_be_bytes())
    }
}

#[cold]
fn virtual_memory_write(
    component_id: ComponentId,
    timestamp: Period,
    registry: ComponentRegistry<'_>,
    destination: usize,
    address_space_id: AddressSpaceId,
    buffer: &[u8],
) -> Result<(), MemoryError> {
    registry
        .interact_dyn(
            component_id,
            timestamp,
            #[inline]
            |component| component.memory_write(destination, address_space_id, buffer),
        )
        .unwrap()
}
