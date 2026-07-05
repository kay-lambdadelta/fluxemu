use std::ops::RangeInclusive;

use fluxemu_range::ContiguousRange;

use crate::{
    machine::Machine,
    memory::{CHUNK_SIZE, MapTarget, MemoryMapCommand, Permissions},
    scheduler::Period,
};

#[test]
fn basic() {
    let (machine, address_space) = Machine::build_test_minimal().address_space(16);

    let (machine, work_ram_path) = machine.memory("work-ram", CHUNK_SIZE * 2, []);
    let machine = machine.map_memory(
        address_space,
        [MemoryMapCommand::Map {
            range: RangeInclusive::from_start_and_length(0, CHUNK_SIZE * 2),
            permissions: Permissions::ALL,
            target: MapTarget::Memory {
                path: work_ram_path,
                subrange: None,
            },
        }],
    );

    let machine = machine.seal().build(());
    let runtime_guard = machine.enter_runtime();

    let mut address_space = runtime_guard.address_space(address_space).unwrap();

    address_space
        .write(0, &Period::default(), &[0xff; CHUNK_SIZE * 2])
        .unwrap();

    let mut buffer = [0; CHUNK_SIZE * 2];
    address_space
        .read(0, &Period::default(), &mut buffer)
        .unwrap();
    assert_eq!(buffer, [0xff; CHUNK_SIZE * 2]);
}

#[test]
fn wraparound() {
    let (machine, address_space) = Machine::build_test_minimal().address_space(8);

    let (machine, work_ram_path) = machine.memory("work-ram", 0x100, []);
    let machine = machine.map_memory(
        address_space,
        [MemoryMapCommand::Map {
            range: RangeInclusive::from_start_and_length(0, 0x100),
            permissions: Permissions::ALL,
            target: MapTarget::Memory {
                path: work_ram_path,
                subrange: None,
            },
        }],
    );

    let machine = machine.seal().build(());
    let runtime_guard = machine.enter_runtime();

    let mut address_space = runtime_guard.address_space(address_space).unwrap();

    address_space
        .write_le_value(0, &Period::default(), 0xffu8)
        .unwrap();
    address_space
        .write(1, &Period::default(), &[0; 0xff])
        .unwrap();

    let mut buffer = [0; 2];
    address_space
        .read(0xff, &Period::default(), &mut buffer)
        .unwrap();
    assert_eq!(buffer, [0x00, 0xff], "{:#?}", address_space);
}
