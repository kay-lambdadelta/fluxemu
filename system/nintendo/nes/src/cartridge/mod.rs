use alloc::boxed::Box;
use core::{error::Error, ops::RangeInclusive};

use bytes::Bytes;
use fluxemu_math::range::ContiguousRange;
use fluxemu_runtime::{
    ResourcePath,
    component::Component,
    machine::builder::ComponentBuilder,
    memory::{Address, AddressSpaceId, MapTarget, MemoryMapCommand, Permissions},
    platform::Platform,
};
use serde::{Deserialize, Serialize};

use crate::{cartridge::ines::NametableMirroring, ppu::NAMETABLE_ADDRESSES};

pub mod ines;
pub mod mapper;

const FIXED_CHR_SIZE: usize = 8 * 1024;

#[derive(Debug)]
pub struct CartParams {
    pub cpu_address_space: AddressSpaceId,
    pub ppu_address_space: AddressSpaceId,
    pub nametables: [ResourcePath; 2],
    pub chr_rom: Option<Bytes>,
    pub prg_rom: Bytes,
    pub prg_ram_size: usize,
    pub prg_nvram_size: usize,
    pub chr_ram_size: usize,
    pub chr_nvram_size: usize,
}

impl CartParams {
    pub fn map_chr_ram<'a, P: Platform, C: Component>(
        &self,
        component_builder: ComponentBuilder<'a, P, C>,
    ) -> Result<ComponentBuilder<'a, P, C>, Box<dyn Error>> {
        let (component_builder, chr_ram_path, size) = if self.chr_nvram_size > 0 {
            let (component_builder, path) =
                component_builder.save_memory("chr-ram", self.chr_nvram_size, []);

            (component_builder, path, self.chr_nvram_size)
        } else if self.chr_ram_size > 0 {
            let (component_builder, path) =
                component_builder.memory("chr-ram", self.chr_ram_size, []);

            (component_builder, path, self.chr_ram_size)
        } else {
            return Err("Cartridge has neither CHR-ROM nor CHR-RAM".into());
        };

        Ok(component_builder.map_memory(
            self.ppu_address_space,
            [MemoryMapCommand::Map {
                range: RangeInclusive::from_start_and_length(0x0000, size),
                permissions: Permissions::ALL,
                target: MapTarget::Memory {
                    path: chr_ram_path,
                    subrange: None,
                },
            }],
        ))
    }

    pub fn map_fixed_chr<'a, P: Platform, C: Component>(
        &self,
        component_builder: ComponentBuilder<'a, P, C>,
    ) -> Result<ComponentBuilder<'a, P, C>, Box<dyn Error>> {
        let Some(chr_rom) = self.chr_rom.clone() else {
            return self.map_chr_ram(component_builder);
        };

        if chr_rom.len() != FIXED_CHR_SIZE {
            return Err("Unbanked CHR-ROM must be exactly 8KB".into());
        }

        Ok(component_builder.map_memory(
            self.ppu_address_space,
            [MemoryMapCommand::immutable_memory(0x0000, chr_rom)],
        ))
    }
}

pub fn rom_bank(
    rom: &Bytes,
    base: Address,
    bank_size: usize,
    bank_index: usize,
) -> MemoryMapCommand {
    let bank_count = rom.len() / bank_size;

    MemoryMapCommand::immutable_memory(
        base,
        rom.slice(RangeInclusive::from_start_and_length(
            (bank_index % bank_count) * bank_size,
            bank_size,
        )),
    )
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Mirroring {
    OneScreenLower,
    OneScreenUpper,
    Vertical,
    Horizontal,
}

impl Mirroring {
    pub fn nametable_commands(
        self,
        nametables: &[ResourcePath; 2],
    ) -> impl Iterator<Item = MemoryMapCommand> {
        let layout = match self {
            Mirroring::OneScreenLower => [0, 0, 0, 0],
            Mirroring::OneScreenUpper => [1, 1, 1, 1],
            Mirroring::Vertical => [0, 1, 0, 1],
            Mirroring::Horizontal => [0, 0, 1, 1],
        };

        NAMETABLE_ADDRESSES
            .into_iter()
            .zip(layout)
            .map(|(range, nametable_index)| MemoryMapCommand::Map {
                range,
                permissions: Permissions::ALL,
                target: MapTarget::Memory {
                    path: nametables[nametable_index].clone(),
                    subrange: None,
                },
            })
    }
}

impl From<NametableMirroring> for Mirroring {
    fn from(mirroring: NametableMirroring) -> Self {
        match mirroring {
            NametableMirroring::Vertical => Mirroring::Vertical,
            NametableMirroring::Horizontal => Mirroring::Horizontal,
        }
    }
}
