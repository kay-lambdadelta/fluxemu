use std::ops::RangeInclusive;

use fluxemu_range::{ContiguousRange, RangeIntersection};
use num::traits::{FromBytes, ops::bytes::NumBytes};

use super::AddressSpace;
use crate::{
    memory::{Address, MemoryError, MemoryErrorType, PageTarget, search::Item},
    scheduler::Period,
};

impl<'a> AddressSpace<'a> {
    /// Force code into the generic read_*_value functions
    #[inline]
    pub(super) fn read_internal<B: NumBytes + ?Sized>(
        &mut self,
        mut address: Address,
        time: Period,
        avoid_side_effects: bool,
        buffer: &mut B,
    ) -> Result<(), MemoryError> {
        let members = self.members_cache.load();
        let buffer = buffer.as_mut();

        // Take a special path for single byte reads
        if buffer.len() == 1 {
            let address_masked = address & self.data.width_mask;

            let Item {
                entry_assigned_range,
                target,
            } = members.read.get(address_masked).ok_or_else(
                #[cold]
                || {
                    let access_range = RangeInclusive::from_start_and_length(address_masked, 1);

                    MemoryError(std::iter::once((access_range, MemoryErrorType::Denied)).collect())
                },
            )?;

            match target {
                PageTarget::Component {
                    mirror_start,
                    component,
                } => {
                    let operation_base = mirror_start.unwrap_or(*entry_assigned_range.start());
                    let offset = address_masked - entry_assigned_range.start();

                    self.runtime
                        .registry()
                        .interact_dyn(
                            *component,
                            time,
                            #[inline]
                            |component| {
                                component.memory_read(
                                    operation_base + offset,
                                    self.data.id,
                                    avoid_side_effects,
                                    buffer,
                                )
                            },
                        )
                        .unwrap()?;
                }
                PageTarget::Memory(bytes) => {
                    let memory_offset = address_masked - entry_assigned_range.start();

                    buffer[0] = bytes[memory_offset];
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
            } in members.read.overlapping(access_range.clone())
            {
                handled = true;

                match target {
                    PageTarget::Component {
                        mirror_start,
                        component,
                    } => {
                        let component_access_range =
                            entry_assigned_range.intersection(&access_range);
                        let offset = component_access_range.start() - entry_assigned_range.start();

                        let operation_base = mirror_start.unwrap_or(*entry_assigned_range.start());

                        let buffer_range = (component_access_range.start() - access_range.start())
                            ..=(component_access_range.end() - access_range.start());
                        let adjusted_buffer = &mut remaining_buffer[buffer_range];

                        self.runtime
                            .registry()
                            .interact_dyn(
                                *component,
                                time,
                                #[inline]
                                |component| {
                                    component.memory_read(
                                        operation_base + offset,
                                        self.data.id,
                                        avoid_side_effects,
                                        adjusted_buffer,
                                    )
                                },
                            )
                            .unwrap()?;
                    }
                    PageTarget::Memory(bytes) => {
                        let memory_access_range = entry_assigned_range.intersection(&access_range);

                        let memory_offset =
                            memory_access_range.start() - entry_assigned_range.start();
                        let buffer_range = (memory_access_range.start() - access_range.start())
                            ..=(memory_access_range.end() - access_range.start());

                        let adjusted_buffer = &mut remaining_buffer[buffer_range];
                        let memory_range = RangeInclusive::from_start_and_length(
                            memory_offset,
                            adjusted_buffer.len(),
                        );

                        adjusted_buffer.copy_from_slice(&bytes[memory_range]);
                    }
                }
            }

            if !handled {
                return Err(MemoryError(
                    std::iter::once((access_range, MemoryErrorType::Denied)).collect(),
                ));
            }

            // Move forward in the buffer
            remaining_buffer = &mut remaining_buffer[chunk_len..];
            address = (address_masked + chunk_len) & self.data.width_mask;
        }

        Ok(())
    }

    /// Step through the memory translation table to fill a buffer
    ///
    /// Contents of the buffer upon failure are usually component specific
    #[inline]
    pub fn read(
        &mut self,
        address: Address,
        current_timestamp: Period,
        buffer: &mut [u8],
    ) -> Result<(), MemoryError> {
        self.read_internal(address, current_timestamp, false, buffer)
    }

    #[inline]
    pub fn read_pure(
        &mut self,
        address: Address,
        current_timestamp: Period,
        buffer: &mut [u8],
    ) -> Result<(), MemoryError> {
        self.read_internal(address, current_timestamp, true, buffer)
    }

    /// Given a location, read a little endian value
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

    /// Given a location, read a little endian value
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

    /// Given a location, read a big endian value
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

    /// Given a location, read a big endian value
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
