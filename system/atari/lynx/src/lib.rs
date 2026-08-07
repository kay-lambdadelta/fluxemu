use std::{ops::RangeInclusive, str::FromStr};

use fluxemu_program::{AtariSystem, RomId, SystemId};
use fluxemu_range::ContiguousRange;
use fluxemu_runtime::{
    machine::builder::{MachineBuilder, SealedMachineBuilder},
    memory::{Address, MapTarget, MemoryMapCommand, Permissions},
    platform::Platform,
};
use fluxemu_system::System;
use mapctl::MapctlConfig;
use num::rational::Ratio;

use crate::suzy::SuzyConfig;

mod mapctl;
mod mikey;
mod suzy;

const SUZY_ADDRESSES: RangeInclusive<Address> = 0xfc00..=0xfcff;
const MIKEY_ADDRESSES: RangeInclusive<Address> = 0xfd00..=0xfdff;
const VECTOR_ADDRESSES: RangeInclusive<Address> = 0xfff8..=0xffff;
const RESERVED_MEMORY_ADDRESS: Address = 0xfff8;
const MAPCTL_ADDRESS: Address = 0xfff9;

#[derive(Debug, Default)]
pub struct AtariLynx;

impl<P: Platform> System<P> for AtariLynx {
    type Quirks = ();

    const ID: SystemId = SystemId::Atari(AtariSystem::Lynx);

    fn build(
        &self,
        _quirks: Self::Quirks,
        machine_builder: MachineBuilder<P>,
    ) -> SealedMachineBuilder<P> {
        // 16 Mhz
        let _base_clock = Ratio::from_integer(16000000);
        let (machine_builder, cpu_address_space) = machine_builder.address_space(16);

        // A good portion of this will be initially shadowed
        let (machine_builder, ram_path) = machine_builder.memory("ram", 0x10000, []);
        let machine_builder = machine_builder.map_memory(
            cpu_address_space,
            [MemoryMapCommand::Map {
                range: RangeInclusive::from_start_and_length(0, 0x10000),
                permissions: Permissions::ALL,
                target: MapTarget::Memory {
                    path: ram_path.clone(),
                    subrange: None,
                },
            }],
        );

        let machine_builder = machine_builder.map_memory(
            cpu_address_space,
            [MemoryMapCommand::Unmap {
                range: RangeInclusive::from_single(RESERVED_MEMORY_ADDRESS),
                permissions: Permissions::ALL,
            }],
        );

        let rom = machine_builder
            .program_manager()
            .load(
                // "[BIOS] Atari Lynx (World).lyx"
                RomId::from_str("e4ed47fae31693e016b081c6bda48da5b70d7ccb").unwrap(),
            )
            .unwrap()
            .unwrap();

        let machine_builder = machine_builder.map_memory(
            cpu_address_space,
            [MemoryMapCommand::immutable_memory(
                0xfe00,
                rom.slice(0x0000..=0x1fff),
            )],
        );

        let (machine_builder, suzy) =
            machine_builder.component("suzy", SuzyConfig { cpu_address_space });

        let (machine_builder, _) = machine_builder.component(
            "mapctl",
            MapctlConfig {
                cpu_address_space,
                ram: ram_path,
                suzy,
                mikey: todo!(),
                vector: todo!(),
            },
        );

        machine_builder.seal()
    }
}
