use std::{
    borrow::Cow,
    collections::HashMap,
    io::{Read, Write},
    ops::RangeInclusive,
};

use bytes::Bytes;
use fluxemu_program::RomId;
use fluxemu_range::{ContiguousRange, RangeIntersection};
use fluxemu_runtime::{
    component::{
        Component, ComponentVersion,
        config::{ComponentConfig, LateContext, LateInitializedData},
    },
    machine::builder::{ComponentBuilder, RomRequirement},
    memory::{Address, AddressSpaceId, MemoryError},
    platform::Platform,
};
use rand::Rng;
use rangemap::RangeInclusiveMap;

#[cfg(test)]
mod tests;

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
    pub initial_contents: RangeInclusiveMap<usize, InitialContents>,
    pub sram: bool,
}

#[derive(Debug)]
pub struct Memory {
    config: MemoryConfig,
    buffer: Vec<u8>,
    roms: HashMap<RomId, Bytes>,
}

impl Component for Memory {
    // The save/snapshot format is just raw bytes so i doubt it will ever change

    fn load_snapshot(
        &mut self,
        version: ComponentVersion,
        reader: &mut dyn Read,
    ) -> Result<(), Box<dyn std::error::Error>> {
        assert_eq!(version, 0);

        reader.read_exact(&mut self.buffer)?;

        Ok(())
    }

    fn store_save(&self, writer: &mut dyn Write) -> Result<(), Box<dyn std::error::Error>> {
        assert!(self.config.sram, "Misbehaving save manager");

        // It's the exact same
        self.store_snapshot(writer)
    }

    fn store_snapshot(&self, writer: &mut dyn Write) -> Result<(), Box<dyn std::error::Error>> {
        writer.write_all(&self.buffer)?;

        Ok(())
    }

    fn memory_read(
        &self,
        address: Address,
        _address_space: AddressSpaceId,
        _avoid_side_effects: bool,
        buffer: &mut [u8],
    ) -> Result<(), MemoryError> {
        let requested_range = address - self.config.assigned_range.start()
            ..=(address - self.config.assigned_range.start() + buffer.len() - 1);

        buffer.copy_from_slice(&self.buffer[requested_range]);

        Ok(())
    }

    fn memory_write(
        &mut self,
        address: Address,
        _address_space: AddressSpaceId,
        buffer: &[u8],
    ) -> Result<(), MemoryError> {
        let requested_range = address - self.config.assigned_range.start()
            ..=(address - self.config.assigned_range.start() + buffer.len() - 1);

        self.buffer[requested_range].copy_from_slice(buffer);

        Ok(())
    }
}

impl<P: Platform> ComponentConfig<P> for MemoryConfig {
    type Component = Memory;

    fn late_initialize(
        _component: &mut Self::Component,
        _data: &LateContext<P>,
    ) -> LateInitializedData<P> {
        Default::default()
    }

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
        let mut roms = HashMap::default();

        for (_, initial_contents) in self.initial_contents.iter() {
            if let InitialContents::Rom(rom_id) = initial_contents
                && !roms.contains_key(rom_id)
            {
                let rom_bytes = component_builder
                    .open_rom(*rom_id, RomRequirement::Required)?
                    .unwrap();

                roms.insert(*rom_id, rom_bytes);
            }
        }

        let mut component = Memory {
            config: self.clone(),
            buffer,
            roms,
        };

        match component_builder.get_save() {
            Some((mut save, 0)) if self.sram => {
                // snapshot and save format are the exact same
                component.load_snapshot(0, &mut save).unwrap();
            }
            Some(_) => return Err("Invalid save version".into()),
            None => {
                component.initialize_buffer();
            }
        }

        match (self.readable, self.writable) {
            (true, true) => {
                component_builder.memory_map_component(assigned_address_space, assigned_range)
            }
            (true, false) => {
                component_builder.memory_map_component_read(assigned_address_space, assigned_range)
            }
            (false, true) => {
                component_builder.memory_map_component_write(assigned_address_space, assigned_range)
            }
            (false, false) => component_builder,
        };

        Ok(component)
    }
}

impl Memory {
    fn initialize_buffer(&mut self) {
        for (range, operation) in self.config.initial_contents.iter() {
            let range = range.start() - self.config.assigned_range.start()
                ..=(range.end() - self.config.assigned_range.start());

            match operation {
                InitialContents::Value(value) => {
                    self.buffer[range.clone()].fill(*value);
                }
                InitialContents::Random => {
                    rand::rng().fill_bytes(&mut self.buffer[range.clone()]);
                }
                InitialContents::Array(value) => {
                    self.buffer[range.clone()].copy_from_slice(value);
                }
                InitialContents::Rom(rom_id) => {
                    let rom = &self.roms[rom_id];

                    let actual_buffer_range = range.intersection(
                        &RangeInclusive::from_start_and_length(*range.start(), rom.len()),
                    );
                    let rom_range = 0..=(actual_buffer_range.end() - actual_buffer_range.start());

                    self.buffer[actual_buffer_range].copy_from_slice(&rom[rom_range]);
                }
            }
        }
    }
}
