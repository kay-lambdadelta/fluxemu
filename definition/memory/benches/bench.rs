use std::hint::black_box;

use criterion::{Criterion, criterion_group, criterion_main};
use fluxemu_definition_memory::{InitialContents, MemoryConfig};
use fluxemu_runtime::{machine::Machine, scheduler::Period};
use rangemap::RangeInclusiveMap;

fn criterion_benchmark(c: &mut Criterion) {
    let mut group = c.benchmark_group(env!("CARGO_PKG_NAME"));

    let (machine, address_space) = Machine::build_test_minimal().address_space(16);

    let (machine, _) = machine.component(
        "ram-memory",
        MemoryConfig {
            readable: true,
            writable: true,
            assigned_range: 0x0000..=0x0fff,
            assigned_address_space: address_space,
            initial_contents: RangeInclusiveMap::from_iter([(
                0x0000..=0x0fff,
                InitialContents::Value(0x00),
            )]),
            sram: false,
        },
    );

    let machine = machine
        .memory_map_buffer_read(address_space, 0x1000..=0x1fff, vec![0u8; 0x1000])
        .seal()
        .unwrap()
        .build(());

    let runtime_guard = machine.enter_runtime();

    let mut address_space = runtime_guard.address_space(address_space).unwrap();

    group.bench_function("read_u8", |b| {
        b.iter(|| {
            black_box(
                address_space
                    .read_le_value::<u8>(black_box(0x0000), Period::default())
                    .unwrap(),
            );
        })
    });
    group.bench_function("read_u16", |b| {
        b.iter(|| {
            black_box(
                address_space
                    .read_le_value::<u16>(black_box(0x0000), Period::default())
                    .unwrap(),
            );
        })
    });
    group.bench_function("read_u32", |b| {
        b.iter(|| {
            black_box(
                address_space
                    .read_le_value::<u32>(black_box(0x0000), Period::default())
                    .unwrap(),
            );
        })
    });
    group.bench_function("read_u64", |b| {
        b.iter(|| {
            black_box(
                address_space
                    .read_le_value::<u64>(black_box(0x0000), Period::default())
                    .unwrap(),
            );
        })
    });

    group.bench_function("read_u8_from_rom", |b| {
        b.iter(|| {
            black_box(
                address_space
                    .read_le_value::<u8>(black_box(0x1000), Period::default())
                    .unwrap(),
            );
        })
    });
    group.bench_function("read_u16_from_rom", |b| {
        b.iter(|| {
            black_box(
                address_space
                    .read_le_value::<u16>(black_box(0x1000), Period::default())
                    .unwrap(),
            );
        })
    });
    group.bench_function("read_u32_from_rom", |b| {
        b.iter(|| {
            black_box(
                address_space
                    .read_le_value::<u32>(black_box(0x1000), Period::default())
                    .unwrap(),
            );
        })
    });
    group.bench_function("read_u64_from_rom", |b| {
        b.iter(|| {
            black_box(
                address_space
                    .read_le_value::<u64>(black_box(0x1000), Period::default())
                    .unwrap(),
            );
        })
    });

    group.bench_function("write_u8", |b| {
        b.iter(|| {
            address_space
                .write_le_value::<u8>(black_box(0x0000), Period::default(), black_box(0))
                .unwrap();
        })
    });
    group.bench_function("write_u16", |b| {
        b.iter(|| {
            address_space
                .write_le_value::<u16>(black_box(0x0000), Period::default(), black_box(0))
                .unwrap();
        })
    });
    group.bench_function("write_u32", |b| {
        b.iter(|| {
            address_space
                .write_le_value::<u32>(black_box(0x0000), Period::default(), black_box(0))
                .unwrap();
        })
    });
    group.bench_function("write_u64", |b| {
        b.iter(|| {
            address_space
                .write_le_value::<u64>(black_box(0x0000), Period::default(), black_box(0))
                .unwrap();
        })
    });
}

criterion_group!(benches, criterion_benchmark);
criterion_main!(benches);
