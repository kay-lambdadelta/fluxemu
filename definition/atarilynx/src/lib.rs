use std::{ops::RangeInclusive, str::FromStr};

use fluxemu_program::RomId;
use fluxemu_runtime::{
    machine::builder::{MachineBuilder, MachineFactory},
    memory::{
        Address,
        component::{InitialContents, MemoryConfig},
    },
    platform::Platform,
};
use mapctl::MapctlConfig;
use num::rational::Ratio;
use rangemap::RangeInclusiveMap;

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
    fn construct<'a>(&self, machine: MachineBuilder<'a, P>) -> MachineBuilder<'a, P> {
        // 16 Mhz
        let _base_clock = Ratio::from_integer(16000000);
        let (machine, cpu_address_space) = machine.address_space(16);

        // A good portion of this will be initially shadowed
        let (machine, ram) = machine.component(
            "ram",
            MemoryConfig {
                readable: true,
                writable: true,
                assigned_range: 0x0000..=0xffff,
                assigned_address_space: cpu_address_space,
                initial_contents: RangeInclusiveMap::from_iter([(
                    0x0000..=0xffff,
                    InitialContents::Value(0xff),
                )]),
                sram: false,
            },
        );

        let machine = machine.memory_unmap(
            cpu_address_space,
            RESERVED_MEMORY_ADDRESS..=RESERVED_MEMORY_ADDRESS,
        );

        let rom = machine
            .program_manager()
            .load(
                // "[BIOS] Atari Lynx (World).lyx"
                RomId::from_str("e4ed47fae31693e016b081c6bda48da5b70d7ccb").unwrap(),
            )
            .unwrap()
            .unwrap();

        let machine = machine.memory_map_buffer_read(
            cpu_address_space,
            0xfe00..=0xffff,
            rom.slice(0x0000..=0x1fff),
        );

        let (machine, suzy) = machine.component("suzy", SuzyConfig { cpu_address_space });

        let (machine, _) = machine.component(
            "mapctl",
            MapctlConfig {
                cpu_address_space,
                ram,
                suzy,
                mikey: todo!(),
                vector: todo!(),
            },
        );

        machine
    }
}
