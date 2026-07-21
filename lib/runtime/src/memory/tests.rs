use std::ops::RangeInclusive;

use bytes::Bytes;
use fluxemu_range::ContiguousRange;

use crate::{
    machine::Machine,
    memory::{CHUNK_SIZE, MapTarget, MemoryErrorType, MemoryMapCommand, Permissions},
    scheduler::Period,
};

#[test]
fn reads_and_writes_sanity() {
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
        .write(0x0000, &Period::ZERO, &[34; CHUNK_SIZE * 2])
        .unwrap();

    let mut buffer = [0; CHUNK_SIZE * 2];
    address_space
        .read::<_, false>(0x0000, &Period::ZERO, &mut buffer)
        .unwrap();
    assert_eq!(buffer, [34; CHUNK_SIZE * 2]);
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
        .write_le_value::<u8>(0x0000, &Period::ZERO, 34)
        .unwrap();
    address_space
        .write(0x0001, &Period::ZERO, &[0; 0xff])
        .unwrap();

    let mut buffer = [0; 2];
    address_space
        .read::<_, false>(0xff, &Period::ZERO, &mut buffer)
        .unwrap();
    assert_eq!(buffer, [0x00, 34]);
}

#[test]
fn immutable_memory_is_read_only() {
    let (machine, address_space) = Machine::build_test_minimal().address_space(16);

    let rom_contents = Bytes::from_static(&[34; 0x34]);
    let machine = machine.map_memory(
        address_space,
        [MemoryMapCommand::immutable_memory(0x0000, rom_contents)],
    );

    let machine = machine.seal().build(());
    let runtime_guard = machine.enter_runtime();
    let mut address_space = runtime_guard.address_space(address_space).unwrap();

    let value: u8 = address_space
        .read_le_value::<_, false>(0x0000, &Period::ZERO)
        .unwrap();
    assert_eq!(value, 34);

    let error = address_space
        .write_le_value::<u8>(0x0000, &Period::ZERO, 0x00)
        .unwrap_err();
    assert_eq!(error.0[0].1, MemoryErrorType::OutOfBus);
}

#[test]
fn mirror_redirects() {
    let (machine, address_space) = Machine::build_test_minimal().address_space(16);

    let (machine, ram_path) = machine.memory("ram", 0x100, []);
    let machine = machine.map_memory(
        address_space,
        [
            MemoryMapCommand::Map {
                range: RangeInclusive::from_start_and_length(0x0000, 0x100),
                permissions: Permissions::ALL,
                target: MapTarget::Memory {
                    path: ram_path,
                    subrange: None,
                },
            },
            MemoryMapCommand::mirror(
                Permissions::ALL,
                RangeInclusive::from_start_and_length(0x0100, 0x100),
                0x0000,
            ),
        ],
    );

    let machine = machine.seal().build(());
    let runtime_guard = machine.enter_runtime();
    let mut address_space = runtime_guard.address_space(address_space).unwrap();

    address_space
        .write_le_value::<u8>(0x0100, &Period::ZERO, 34)
        .unwrap();
    let value: u8 = address_space
        .read_le_value::<_, false>(0x0000, &Period::ZERO)
        .unwrap();
    assert_eq!(value, 34);

    address_space
        .write_le_value::<u8>(0x0001, &Period::ZERO, 0x34)
        .unwrap();
    let mirrored_value: u8 = address_space
        .read_le_value::<_, false>(0x0101, &Period::ZERO)
        .unwrap();
    assert_eq!(mirrored_value, 0x34);
}

#[test]
fn unmap() {
    let (machine, address_space) = Machine::build_test_minimal().address_space(16);

    let (machine, ram_path) = machine.memory("ram", 0x100, []);
    let machine = machine.map_memory(
        address_space,
        [MemoryMapCommand::Map {
            range: RangeInclusive::from_start_and_length(0x0000, 0x100),
            permissions: Permissions::ALL,
            target: MapTarget::Memory {
                path: ram_path,
                subrange: None,
            },
        }],
    );

    let machine = machine.seal().build(());
    let runtime_guard = machine.enter_runtime();
    let mut address_space = runtime_guard.address_space(address_space).unwrap();

    address_space
        .write_le_value(0x0000, &Period::ZERO, 0x11u8)
        .unwrap();

    address_space.remap(
        &Period::ZERO,
        [MemoryMapCommand::Unmap {
            range: RangeInclusive::from_start_and_length(0x0000, 0x100),
            permissions: Permissions::ALL,
        }],
    );

    let error = address_space
        .read_le_value::<u8, false>(0x0000, &Period::ZERO)
        .unwrap_err();
    assert_eq!(error.0[0].1, MemoryErrorType::OutOfBus);
}

#[test]
fn permissions_are_enforced() {
    let (machine, address_space) = Machine::build_test_minimal().address_space(16);

    let (machine, read_only_path) = machine.memory("read-only", 0x10, []);
    let (machine, write_only_path) = machine.memory("write-only", 0x10, []);

    let machine = machine.map_memory(
        address_space,
        [
            MemoryMapCommand::Map {
                range: RangeInclusive::from_start_and_length(0x0000, 0x10),
                permissions: Permissions::WRITE,
                target: MapTarget::Memory {
                    path: write_only_path,
                    subrange: None,
                },
            },
            MemoryMapCommand::Map {
                range: RangeInclusive::from_start_and_length(0x0010, 0x10),
                permissions: Permissions::READ,
                target: MapTarget::Memory {
                    path: read_only_path,
                    subrange: None,
                },
            },
        ],
    );

    let machine = machine.seal().build(());
    let runtime_guard = machine.enter_runtime();
    let mut address_space = runtime_guard.address_space(address_space).unwrap();

    address_space
        .write_le_value::<u8>(0x0000, &Period::ZERO, 34)
        .unwrap();
    let read_error = address_space
        .read_le_value::<u8, false>(0x0000, &Period::ZERO)
        .unwrap_err();
    assert_eq!(read_error.0[0].1, MemoryErrorType::OutOfBus);

    let write_error = address_space
        .write_le_value::<u8>(0x0010, &Period::ZERO, 34)
        .unwrap_err();
    assert_eq!(write_error.0[0].1, MemoryErrorType::OutOfBus);
}

#[test]
fn initial_contents_are_applied() {
    let (machine, address_space) = Machine::build_test_minimal().address_space(16);

    let (machine, rom_path) = machine.memory(
        "rom",
        0x10,
        [(
            RangeInclusive::from_start_and_length(0, 4),
            Bytes::from_static(&[1, 2, 3, 4]),
        )],
    );

    let machine = machine.map_memory(
        address_space,
        [MemoryMapCommand::Map {
            range: RangeInclusive::from_start_and_length(0x0000, 0x10),
            permissions: Permissions::READ,
            target: MapTarget::Memory {
                path: rom_path,
                subrange: None,
            },
        }],
    );

    let machine = machine.seal().build(());
    let runtime_guard = machine.enter_runtime();
    let mut address_space = runtime_guard.address_space(address_space).unwrap();

    let mut buffer = [0; 8];
    address_space
        .read::<_, false>(0x0000, &Period::ZERO, &mut buffer)
        .unwrap();

    assert_eq!(buffer, [1, 2, 3, 4, 0, 0, 0, 0]);
}
