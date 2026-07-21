use std::{hint::black_box, ops::RangeInclusive, sync::Arc};

use criterion::{Criterion, criterion_group, criterion_main};
use fluxemu_range::ContiguousRange;
use fluxemu_runtime::{
    machine::Machine,
    memory::{AddressSpaceId, MapTarget, MemoryMapCommand, Permissions},
    scheduler::Period,
};

fn build_machine() -> (Arc<Machine>, AddressSpaceId) {
    let (machine, address_space_id) = Machine::build_test_minimal().address_space(16);

    let (machine, ram_path) = machine.memory("ram-memory", 0x1000, []);

    let machine = machine
        .map_memory(
            address_space_id,
            [
                MemoryMapCommand::Map {
                    range: RangeInclusive::from_start_and_length(0, 0x1000),
                    target: MapTarget::Memory {
                        path: ram_path,
                        subrange: None,
                    },
                    permissions: Permissions::ALL,
                },
                MemoryMapCommand::Map {
                    range: RangeInclusive::from_start_and_length(0x1000, 0x1000),
                    target: MapTarget::ImmutableMemory(vec![0; 0x1000].into()),
                    permissions: Permissions::READ,
                },
            ],
        )
        .seal()
        .build(());

    (machine, address_space_id)
}

fn bench_reads(c: &mut Criterion) {
    let (machine, address_space_id) = build_machine();
    let runtime_guard = machine.enter_runtime();
    let mut address_space = runtime_guard.address_space(address_space_id).unwrap();

    let mut group = c.benchmark_group(format!("{}/memory/read", env!("CARGO_PKG_NAME")));

    group.bench_function("u8", |b| {
        b.iter(|| {
            black_box(
                address_space
                    .read_le_value::<u8, false>(black_box(0x0000), &Period::default())
                    .unwrap(),
            )
        });
    });

    group.bench_function("u16", |b| {
        b.iter(|| {
            black_box(
                address_space
                    .read_le_value::<u16, false>(black_box(0x0000), &Period::default())
                    .unwrap(),
            )
        });
    });

    group.bench_function("u32", |b| {
        b.iter(|| {
            black_box(
                address_space
                    .read_le_value::<u32, false>(black_box(0x0000), &Period::default())
                    .unwrap(),
            )
        });
    });

    group.bench_function("u64", |b| {
        b.iter(|| {
            black_box(
                address_space
                    .read_le_value::<u64, false>(black_box(0x0000), &Period::default())
                    .unwrap(),
            )
        });
    });

    group.bench_function("u8_from_rom", |b| {
        b.iter(|| {
            black_box(
                address_space
                    .read_le_value::<u8, false>(black_box(0x1000), &Period::default())
                    .unwrap(),
            )
        });
    });

    group.bench_function("u16_from_rom", |b| {
        b.iter(|| {
            black_box(
                address_space
                    .read_le_value::<u16, false>(black_box(0x1000), &Period::default())
                    .unwrap(),
            )
        });
    });

    group.bench_function("u32_from_rom", |b| {
        b.iter(|| {
            black_box(
                address_space
                    .read_le_value::<u32, false>(black_box(0x1000), &Period::default())
                    .unwrap(),
            )
        });
    });

    group.bench_function("u64_from_rom", |b| {
        b.iter(|| {
            black_box(
                address_space
                    .read_le_value::<u64, false>(black_box(0x1000), &Period::default())
                    .unwrap(),
            )
        });
    });

    group.finish();
}

fn bench_writes(c: &mut Criterion) {
    let (machine, address_space_id) = build_machine();
    let runtime_guard = machine.enter_runtime();
    let mut address_space = runtime_guard.address_space(address_space_id).unwrap();

    let mut group = c.benchmark_group(format!("{}/memory/write", env!("CARGO_PKG_NAME")));

    group.bench_function("u8", |b| {
        b.iter(|| {
            address_space
                .write_le_value::<u8>(black_box(0x0000), &Period::default(), black_box(0))
                .unwrap();
        });
    });

    group.bench_function("u16", |b| {
        b.iter(|| {
            address_space
                .write_le_value::<u16>(black_box(0x0000), &Period::default(), black_box(0))
                .unwrap();
        });
    });

    group.bench_function("u32", |b| {
        b.iter(|| {
            address_space
                .write_le_value::<u32>(black_box(0x0000), &Period::default(), black_box(0))
                .unwrap();
        });
    });

    group.bench_function("u64", |b| {
        b.iter(|| {
            address_space
                .write_le_value::<u64>(black_box(0x0000), &Period::default(), black_box(0))
                .unwrap();
        });
    });

    group.finish();
}

criterion_group!(benches, bench_reads, bench_writes);
criterion_main!(benches);
