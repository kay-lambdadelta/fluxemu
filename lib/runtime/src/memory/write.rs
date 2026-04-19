use std::ops::RangeInclusive;

use fluxemu_range::{ContiguousRange, RangeIntersection};
use num::traits::{ToBytes, ops::bytes::NumBytes};

use super::AddressSpace;
use crate::{
    memory::{Address, MemoryError, MemoryErrorType, PageTarget, search::Item},
    scheduler::Period,
};

impl<'a> AddressSpace<'a> {
    #[inline]
    pub(super) fn write_internal<B: NumBytes + ?Sized>(
        &mut self,
        mut address: Address,
        time: Period,
        buffer: &B,
    ) -> Result<(), MemoryError> {
        let members = self.members_cache.load();
        let buffer = buffer.as_ref();

        // Take a special path for single byte reads
        if buffer.len() == 1 {
            let address_masked = address & self.data.width_mask;

            let Item {
                entry_assigned_range,
                target,
            } = members.write.get(address_masked).ok_or_else(
                #[cold]
                || {
                    let access_range = RangeInclusive::from_start_and_length(address_masked, 1);

                    MemoryError(std::iter::once((access_range, MemoryErrorType::Denied)).collect())
                },
            )?;

            match target {
                PageTarget::Component {
                    destination_start,
                    component,
                } => {
                    let offset = address_masked - entry_assigned_range.start();

                    self.registry
                        .interact_dyn(
                            *component,
                            time,
                            #[inline]
                            |component| {
                                component.memory_write(
                                    destination_start + offset,
                                    self.data.id,
                                    buffer,
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

            for Item {
                entry_assigned_range,
                target,
            } in members.write.overlapping(access_range.clone())
            {
                handled = true;

                match target {
                    PageTarget::Component {
                        destination_start,
                        component,
                    } => {
                        let component_access_range =
                            entry_assigned_range.intersection(&access_range);
                        let offset = component_access_range.start() - entry_assigned_range.start();
                        let buffer_range = (component_access_range.start() - access_range.start())
                            ..=(component_access_range.end() - access_range.start());
                        let adjusted_buffer = &remaining_buffer[buffer_range];

                        self.registry
                            .interact_dyn(
                                *component,
                                time,
                                #[inline]
                                |component| {
                                    component.memory_write(
                                        destination_start + offset,
                                        self.data.id,
                                        adjusted_buffer,
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
