use std::{ops::RangeInclusive, sync::atomic::Ordering};

use fluxemu_range::{ContiguousRange, RangeIntersection};
use num::traits::{ToBytes, ops::bytes::NumBytes};

use super::AddressSpace;
use crate::{
    component::Component,
    memory::{
        Address, AddressSpaceId, MemoryError, MemoryErrorType, PageEntry, PageTarget,
        component::Memory,
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
        let members = self
            .data
            .members
            .load(Ordering::Acquire, &self.guard)
            .as_ref()
            .unwrap();

        let buffer = buffer.as_ref();

        // Take a special path for single byte writes
        if buffer.len() == 1 {
            let address_masked = address & self.data.width_mask;

            // SAFETY: We just masked the address to fit within the table
            let PageEntry {
                range: entry_assigned_range,
                target,
            } = unsafe { members.write.get(address_masked) }.ok_or_else(
                #[cold]
                || {
                    let access_range = RangeInclusive::from_start_and_length(address_masked, 1);

                    MemoryError(std::iter::once((access_range, MemoryErrorType::Denied)).collect())
                },
            )?;

            match target {
                PageTarget::Component {
                    destination_start,
                    component_id,
                    is_standard_memory,
                } => {
                    let offset = address_masked - entry_assigned_range.start();
                    let destination = destination_start + offset;

                    self.registry
                        .interact_dyn(
                            *component_id,
                            timestamp,
                            #[inline]
                            |component| {
                                perform_write(
                                    destination,
                                    self.data.id,
                                    buffer,
                                    *is_standard_memory,
                                    component,
                                )
                            },
                        )
                        .unwrap()?;
                }
                PageTarget::Memory(_) => {
                    unreachable!()
                }
            }

            return Ok(());
        }

        let mut remaining_buffer = buffer;

        while !remaining_buffer.is_empty() {
            let address_masked = address & self.data.width_mask;
            let end_address = address_masked + remaining_buffer.len() - 1;

            let chunk_len = if end_address > self.data.width_mask {
                // Wraparound
                self.data.width_mask - address_masked + 1
            } else {
                remaining_buffer.len()
            };

            let access_range = RangeInclusive::from_start_and_length(address_masked, chunk_len);
            let mut handled = false;

            for PageEntry {
                range: entry_assigned_range,
                target,
            } in members.write.overlapping(access_range.clone())
            {
                handled = true;

                match target {
                    PageTarget::Component {
                        destination_start,
                        component_id,
                        is_standard_memory,
                    } => {
                        let component_access_range =
                            entry_assigned_range.intersection(&access_range);
                        let offset = component_access_range.start() - entry_assigned_range.start();
                        let buffer_range = (component_access_range.start() - access_range.start())
                            ..=(component_access_range.end() - access_range.start());
                        let adjusted_buffer = &remaining_buffer[buffer_range];

                        let destination = destination_start + offset;

                        self.registry
                            .interact_dyn(
                                *component_id,
                                timestamp,
                                #[inline]
                                |component| {
                                    perform_write(
                                        destination,
                                        self.data.id,
                                        adjusted_buffer,
                                        *is_standard_memory,
                                        component,
                                    )
                                },
                            )
                            .unwrap()?;
                    }
                    PageTarget::Memory(_) => {
                        unreachable!()
                    }
                }
            }

            if !handled {
                return Err(MemoryError(
                    std::iter::once((access_range, MemoryErrorType::Denied)).collect(),
                ));
            }

            // Move forward in the buffer
            remaining_buffer = &remaining_buffer[chunk_len..];
            address = (address_masked + chunk_len) & self.data.width_mask;
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

#[inline]
fn perform_write(
    destination: usize,
    address_space_id: AddressSpaceId,
    buffer: &[u8],
    is_standard_memory: bool,
    component: &mut dyn Component,
) -> Result<(), MemoryError> {
    // We perform a manual devirtualization here because it can often prevent a indirect call
    // and speaking to the internals to the standard memory component specifically, a memcpy call

    if is_standard_memory {
        // SAFETY: In `commit` is_standard_memory is set based upon the typeid of the component
        //
        // This is basically doing a stable `downcast_unchecked`
        let component = unsafe { &mut *(std::ptr::from_mut(component) as *mut Memory) };

        component.memory_write(destination, address_space_id, buffer)
    } else {
        component.memory_write(destination, address_space_id, buffer)
    }
}
