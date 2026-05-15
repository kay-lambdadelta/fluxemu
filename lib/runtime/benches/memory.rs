use std::{hint::black_box, sync::Arc};

use criterion::{Criterion, criterion_group, criterion_main};
use fluxemu_runtime::{
    machine::Machine,
    memory::{
        AddressSpaceId,
        component::{InitialContents, MemoryConfig},
    },
    scheduler::Period,
};
use rangemap::RangeInclusiveMap;

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

fn bench_reads(c: &mut Criterion) {
    let (machine, address_space_id) = build_machine();
    let runtime_guard = machine.enter_runtime();
    let mut address_space = runtime_guard.address_space(address_space_id).unwrap();

    let mut group = c.benchmark_group(format!("{}/memory/read", env!("CARGO_PKG_NAME")));

    group.bench_function("u8", |b| {
        b.iter(|| {
            black_box(
                address_space
                    .read_le_value::<u8>(black_box(0x0000), Period::default())
                    .unwrap(),
            )
        });
    });

    group.bench_function("u16", |b| {
        b.iter(|| {
            black_box(
                address_space
                    .read_le_value::<u16>(black_box(0x0000), Period::default())
                    .unwrap(),
            )
        });
    });

    group.bench_function("u32", |b| {
        b.iter(|| {
            black_box(
                address_space
                    .read_le_value::<u32>(black_box(0x0000), Period::default())
                    .unwrap(),
            )
        });
    });

    group.bench_function("u64", |b| {
        b.iter(|| {
            black_box(
                address_space
                    .read_le_value::<u64>(black_box(0x0000), Period::default())
                    .unwrap(),
            )
        });
    });

    group.bench_function("u8_from_rom", |b| {
        b.iter(|| {
            black_box(
                address_space
                    .read_le_value::<u8>(black_box(0x1000), Period::default())
                    .unwrap(),
            )
        });
    });

    group.bench_function("u16_from_rom", |b| {
        b.iter(|| {
            black_box(
                address_space
                    .read_le_value::<u16>(black_box(0x1000), Period::default())
                    .unwrap(),
            )
        });
    });

    group.bench_function("u32_from_rom", |b| {
        b.iter(|| {
            black_box(
                address_space
                    .read_le_value::<u32>(black_box(0x1000), Period::default())
                    .unwrap(),
            )
        });
    });

    group.bench_function("u64_from_rom", |b| {
        b.iter(|| {
            black_box(
                address_space
                    .read_le_value::<u64>(black_box(0x1000), Period::default())
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
                .write_le_value::<u8>(black_box(0x0000), Period::default(), black_box(0))
                .unwrap();
        });
    });

    group.bench_function("u16", |b| {
        b.iter(|| {
            address_space
                .write_le_value::<u16>(black_box(0x0000), Period::default(), black_box(0))
                .unwrap();
        });
    });

    group.bench_function("u32", |b| {
        b.iter(|| {
            address_space
                .write_le_value::<u32>(black_box(0x0000), Period::default(), black_box(0))
                .unwrap();
        });
    });

    group.bench_function("u64", |b| {
        b.iter(|| {
            address_space
                .write_le_value::<u64>(black_box(0x0000), Period::default(), black_box(0))
                .unwrap();
        });
    });

    group.finish();
}

criterion_group!(benches, bench_reads, bench_writes);
criterion_main!(benches);
