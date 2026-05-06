use std::{hint::black_box, sync::Arc};

use divan::Bencher;
use fluxemu_runtime::{
    machine::Machine,
    memory::{
        AddressSpaceId,
        component::{InitialContents, MemoryConfig},
    },
    scheduler::Period,
};
use rangemap::RangeInclusiveMap;

fn main() {
    divan::main();
}

fn build_machine() -> (Arc<Machine>, AddressSpaceId) {
    let (machine, address_space_id) = Machine::build_test_minimal().address_space(16);
    let (machine, _) = machine.component(
        "ram-memory",
        MemoryConfig {
            readable: true,
            writable: true,
            assigned_range: 0x0000..=0x0fff,
            assigned_address_space: address_space_id,
            initial_contents: RangeInclusiveMap::from_iter([(
                0x0000..=0x0fff,
                InitialContents::Value(0x00),
            )]),
            sram: false,
        },
    );

    let machine = machine
        .memory_map_buffer_read(address_space_id, 0x1000..=0x1fff, vec![0u8; 0x1000])
        .seal()
        .build(());

    (machine, address_space_id)
}

#[divan::bench(sample_size = 1_000_000)]
fn read_u8(bencher: Bencher) {
    let (machine, address_space_id) = build_machine();
    let runtime_guard = machine.enter_runtime();
    let mut address_space = runtime_guard.address_space(address_space_id).unwrap();

    bencher.bench_local(|| {
        black_box(
            address_space
                .read_le_value::<u8>(black_box(0x0000), Period::default())
                .unwrap(),
        );
    });
}

#[divan::bench(sample_size = 1_000_000)]
fn read_u16(bencher: Bencher) {
    let (machine, address_space_id) = build_machine();
    let runtime_guard = machine.enter_runtime();
    let mut address_space = runtime_guard.address_space(address_space_id).unwrap();

    bencher.bench_local(|| {
        black_box(
            address_space
                .read_le_value::<u16>(black_box(0x0000), Period::default())
                .unwrap(),
        );
    });
}

#[divan::bench(sample_size = 1_000_000)]
fn read_u32(bencher: Bencher) {
    let (machine, address_space_id) = build_machine();
    let runtime_guard = machine.enter_runtime();
    let mut address_space = runtime_guard.address_space(address_space_id).unwrap();

    bencher.bench_local(|| {
        black_box(
            address_space
                .read_le_value::<u32>(black_box(0x0000), Period::default())
                .unwrap(),
        );
    });
}

#[divan::bench(sample_size = 1_000_000)]
fn read_u64(bencher: Bencher) {
    let (machine, address_space_id) = build_machine();
    let runtime_guard = machine.enter_runtime();
    let mut address_space = runtime_guard.address_space(address_space_id).unwrap();

    bencher.bench_local(|| {
        black_box(
            address_space
                .read_le_value::<u64>(black_box(0x0000), Period::default())
                .unwrap(),
        );
    });
}

#[divan::bench(sample_size = 1_000_000)]
fn read_u8_from_rom(bencher: Bencher) {
    let (machine, address_space_id) = build_machine();
    let runtime_guard = machine.enter_runtime();
    let mut address_space = runtime_guard.address_space(address_space_id).unwrap();

    bencher.bench_local(|| {
        black_box(
            address_space
                .read_le_value::<u8>(black_box(0x1000), Period::default())
                .unwrap(),
        );
    });
}

#[divan::bench(sample_size = 1_000_000)]
fn read_u16_from_rom(bencher: Bencher) {
    let (machine, address_space_id) = build_machine();
    let runtime_guard = machine.enter_runtime();
    let mut address_space = runtime_guard.address_space(address_space_id).unwrap();

    bencher.bench_local(|| {
        black_box(
            address_space
                .read_le_value::<u16>(black_box(0x1000), Period::default())
                .unwrap(),
        );
    });
}

#[divan::bench(sample_size = 1_000_000)]
fn read_u32_from_rom(bencher: Bencher) {
    let (machine, address_space_id) = build_machine();
    let runtime_guard = machine.enter_runtime();
    let mut address_space = runtime_guard.address_space(address_space_id).unwrap();

    bencher.bench_local(|| {
        black_box(
            address_space
                .read_le_value::<u32>(black_box(0x1000), Period::default())
                .unwrap(),
        );
    });
}

#[divan::bench(sample_size = 1_000_000)]
fn read_u64_from_rom(bencher: Bencher) {
    let (machine, address_space_id) = build_machine();
    let runtime_guard = machine.enter_runtime();
    let mut address_space = runtime_guard.address_space(address_space_id).unwrap();

    bencher.bench_local(|| {
        black_box(
            address_space
                .read_le_value::<u64>(black_box(0x1000), Period::default())
                .unwrap(),
        );
    });
}

#[divan::bench(sample_size = 1_000_000)]
fn write_u8(bencher: Bencher) {
    let (machine, address_space_id) = build_machine();
    let runtime_guard = machine.enter_runtime();
    let mut address_space = runtime_guard.address_space(address_space_id).unwrap();

    bencher.bench_local(|| {
        address_space
            .write_le_value::<u8>(black_box(0x0000), Period::default(), black_box(0))
            .unwrap();
    });
}

#[divan::bench(sample_size = 1_000_000)]
fn write_u16(bencher: Bencher) {
    let (machine, address_space_id) = build_machine();
    let runtime_guard = machine.enter_runtime();
    let mut address_space = runtime_guard.address_space(address_space_id).unwrap();

    bencher.bench_local(|| {
        address_space
            .write_le_value::<u16>(black_box(0x0000), Period::default(), black_box(0))
            .unwrap();
    });
}

#[divan::bench(sample_size = 1_000_000)]
fn write_u32(bencher: Bencher) {
    let (machine, address_space_id) = build_machine();
    let runtime_guard = machine.enter_runtime();
    let mut address_space = runtime_guard.address_space(address_space_id).unwrap();

    bencher.bench_local(|| {
        address_space
            .write_le_value::<u32>(black_box(0x0000), Period::default(), black_box(0))
            .unwrap();
    });
}

#[divan::bench(sample_size = 1_000_000)]
fn write_u64(bencher: Bencher) {
    let (machine, address_space_id) = build_machine();
    let runtime_guard = machine.enter_runtime();
    let mut address_space = runtime_guard.address_space(address_space_id).unwrap();

    bencher.bench_local(|| {
        address_space
            .write_le_value::<u64>(black_box(0x0000), Period::default(), black_box(0))
            .unwrap();
    });
}
