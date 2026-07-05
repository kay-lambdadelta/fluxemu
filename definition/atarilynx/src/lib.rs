use std::{ops::RangeInclusive, str::FromStr};

use fluxemu_program::RomId;
use fluxemu_range::ContiguousRange;
use fluxemu_runtime::{
    machine::builder::{MachineBuilder, MachineFactory},
    memory::{Address, MapTarget, MemoryMapCommand, Permissions},
    platform::Platform,
};
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

impl<P: Platform> MachineFactory<P> for AtariLynx {
    fn construct<'a>(&self, machine: MachineBuilder<P>) -> MachineBuilder<P> {
        // 16 Mhz
        let _base_clock = Ratio::from_integer(16000000);
        let (machine, cpu_address_space) = machine.address_space(16);

        // A good portion of this will be initially shadowed
        let (machine, ram_path) = machine.memory("ram", 0x10000, []);
        let machine = machine.map_memory(
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

        let machine = machine.map_memory(
            cpu_address_space,
            [MemoryMapCommand::Unmap {
                range: RangeInclusive::from_single(RESERVED_MEMORY_ADDRESS),
                permissions: Permissions::ALL,
            }],
        );

        let rom = machine
            .program_manager()
            .load(
                // "[BIOS] Atari Lynx (World).lyx"
                RomId::from_str("e4ed47fae31693e016b081c6bda48da5b70d7ccb").unwrap(),
            )
            .unwrap()
            .unwrap();

        let machine = machine.map_memory(
            cpu_address_space,
            [MemoryMapCommand::immutable_memory(
                0xfe00,
                rom.slice(0x0000..=0x1fff),
            )],
        );

        let (machine, suzy) = machine.component("suzy", SuzyConfig { cpu_address_space });

        let (machine, _) = machine.component(
            "mapctl",
            MapctlConfig {
                cpu_address_space,
                ram: ram_path,
                suzy,
                mikey: todo!(),
                vector: todo!(),
            },
        );

        machine
    }
}
