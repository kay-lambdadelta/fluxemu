use std::{
    borrow::Cow,
    io::{Read, Write},
    ops::RangeInclusive,
};

use fluxemu_program::RomId;
use fluxemu_range::{ContiguousRange, RangeIntersection};
use rand::Rng;
use rangemap::RangeInclusiveMap;

use crate::{
    Platform,
    component::{Component, config::ComponentConfig},
    machine::builder::{ComponentBuilder, RomRequirement},
    memory::{Address, AddressSpaceId, MemoryError},
    persistence::PersistanceFormatVersion,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InitialContents {
    Value(u8),
    Array(Cow<'static, [u8]>),
    Rom(RomId),
    Random,
}

#[derive(Debug, Clone)]
pub struct MemoryConfig {
    pub readable: bool,
    pub writable: bool,
    pub assigned_range: RangeInclusive<Address>,
    pub assigned_address_space: AddressSpaceId,
    pub initial_contents: RangeInclusiveMap<Address, InitialContents>,
    pub sram: bool,
}

#[derive(Debug)]
pub struct Memory {
    buffer: Box<[u8]>,
    base: Address,
}

impl Component for Memory {
    type Event = ();

    fn load_snapshot(
        &mut self,
        _version: PersistanceFormatVersion,
        reader: &mut dyn Read,
    ) -> Result<(), Box<dyn std::error::Error>> {
        // Read out base
        let mut base = [0; size_of::<u64>()];
        reader.read_exact(&mut base)?;
        self.base = u64::from_le_bytes(base).try_into()?;

        // Read out buffer
        reader.read_exact(&mut self.buffer)?;

        Ok(())
    }

    fn store_save(&self, writer: &mut dyn Write) -> Result<(), Box<dyn std::error::Error>> {
        // It's the exact same
        self.store_snapshot(writer)
    }

    fn store_snapshot(&self, writer: &mut dyn Write) -> Result<(), Box<dyn std::error::Error>> {
        writer.write_all(&self.base.to_le_bytes())?;
        writer.write_all(&self.buffer)?;

        Ok(())
    }

    #[inline]
    fn memory_read(
        &mut self,
        address: Address,
        _address_space: AddressSpaceId,
        _avoid_side_effects: bool,
        buffer: &mut [u8],
    ) -> Result<(), MemoryError> {
        let requested_range =
            RangeInclusive::from_start_and_length(address - self.base, buffer.len());

        buffer.copy_from_slice(&self.buffer[requested_range]);

        Ok(())
    }

    #[inline]
    fn memory_write(
        &mut self,
        address: Address,
        _address_space: AddressSpaceId,
        buffer: &[u8],
    ) -> Result<(), MemoryError> {
        let requested_range =
            RangeInclusive::from_start_and_length(address - self.base, buffer.len());

        self.buffer[requested_range].copy_from_slice(buffer);

        Ok(())
    }

    fn memory_rebase(&mut self, base: Address) {
        self.base = base;
    }
}

impl<P: Platform> ComponentConfig<P> for MemoryConfig {
    type Component = Memory;

    const CURRENT_SNAPSHOT_VERSION: PersistanceFormatVersion = 0;

    fn build_component(
        self,
        component_builder: ComponentBuilder<'_, '_, P, Self::Component>,
    ) -> Result<Self::Component, Box<dyn std::error::Error>> {
        if self.assigned_range.is_empty() {
            return Err("Memory assigned must be non-empty".into());
        }

        let buffer_size = self.assigned_range.clone().count();
        let buffer = vec![0; buffer_size];
        let assigned_range = self.assigned_range.clone();
        let assigned_address_space = self.assigned_address_space;

        let mut component = Memory {
            buffer: buffer.into_boxed_slice(),
            base: *self.assigned_range.start(),
        };

        match component_builder.get_save() {
            Some((mut save, 0)) if self.sram => {
                // snapshot and save format are the exact same
                component.load_snapshot(0, &mut save).unwrap();
            }
            Some(_) => return Err("Invalid save version".into()),
            None => {
                for (range, operation) in self.initial_contents.iter() {
                    let range = range.start() - self.assigned_range.start()
                        ..=(range.end() - self.assigned_range.start());

                    match operation {
                        InitialContents::Value(value) => {
                            component.buffer[range.clone()].fill(*value);
                        }
                        InitialContents::Random => {
                            rand::rng().fill_bytes(&mut component.buffer[range.clone()]);
                        }
                        InitialContents::Array(value) => {
                            component.buffer[range.clone()].copy_from_slice(value);
                        }
                        InitialContents::Rom(rom_id) => {
                            let rom_bytes = component_builder
                                .open_rom(*rom_id, RomRequirement::Required)?
                                .unwrap();

                            let actual_buffer_range =
                                range.intersection(&RangeInclusive::from_start_and_length(
                                    *range.start(),
                                    rom_bytes.len(),
                                ));

                            let rom_range =
                                0..=(actual_buffer_range.end() - actual_buffer_range.start());

                            component.buffer[actual_buffer_range]
                                .copy_from_slice(&rom_bytes[rom_range]);
                        }
                    }
                }
            }
        }

        let component_builder =
            match (self.readable, self.writable) {
                (true, true) => {
                    component_builder.memory_map_component(assigned_address_space, assigned_range)
                }
                (true, false) => component_builder
                    .memory_map_component_read(assigned_address_space, assigned_range),
                (false, true) => component_builder
                    .memory_map_component_write(assigned_address_space, assigned_range),
                (false, false) => component_builder,
            };

        if self.sram {
            component_builder.save_version(0);
        }

        Ok(component)
    }
}
