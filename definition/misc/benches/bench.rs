use std::hint::black_box;

use bytes::Bytes;
use criterion::{Criterion, criterion_group, criterion_main};
use fluxemu_definition_misc::memory::standard::{
    StandardMemoryConfig, StandardMemoryInitialContents,
};
use fluxemu_runtime::machine::Machine;
use rangemap::RangeInclusiveMap;

fn criterion_benchmark(c: &mut Criterion) {
    let mut group = c.benchmark_group(env!("CARGO_PKG_NAME"));

    let (machine, address_space) = Machine::build_test_minimal().insert_address_space(16);

    let (machine, _) = machine.insert_component(
        "ram-memory",
        StandardMemoryConfig {
            readable: true,
            writable: true,
            assigned_range: 0x0000..=0x0fff,
            assigned_address_space: address_space,
            initial_contents: RangeInclusiveMap::from_iter([(
                0x0000..=0x0fff,
                StandardMemoryInitialContents::Value(0x00),
            )]),
            sram: false,
        },
    );
    let (machine, buffer_path) = machine.memory_register_buffer(
        address_space,
        "rom-memory",
        Bytes::from_owner(vec![0u8; 0x1000]),
    );
    let machine = machine.memory_map_buffer_read(address_space, 0x1000..=0x1fff, &buffer_path);

    let machine = machine.build(());
    let address_space = machine.address_spaces(address_space).unwrap();
    let mut address_space_cache = address_space.cache();

    group.bench_function("read_u8", |b| {
        b.iter(|| {
            black_box(
                address_space
                    .read_le_value::<u8>(black_box(0x0000), machine.now(), None)
                    .unwrap(),
            );
        })
    });
    group.bench_function("read_u16", |b| {
        b.iter(|| {
            black_box(
                address_space
                    .read_le_value::<u16>(black_box(0x0000), machine.now(), None)
                    .unwrap(),
            );
        })
    });
    group.bench_function("read_u32", |b| {
        b.iter(|| {
            black_box(
                address_space
                    .read_le_value::<u32>(black_box(0x0000), machine.now(), None)
                    .unwrap(),
            );
        })
    });
    group.bench_function("read_u64", |b| {
        b.iter(|| {
            black_box(
                address_space
                    .read_le_value::<u64>(black_box(0x0000), machine.now(), None)
                    .unwrap(),
            );
        })
    });
    group.bench_function("read_u8_with_cache", |b| {
        b.iter(|| {
            black_box(
                address_space
                    .read_le_value::<u8>(
                        black_box(0x0000),
                        machine.now(),
                        Some(&mut address_space_cache),
                    )
                    .unwrap(),
            );
        })
    });
    group.bench_function("read_u16_with_cache", |b| {
        b.iter(|| {
            black_box(
                address_space
                    .read_le_value::<u16>(
                        black_box(0x0000),
                        machine.now(),
                        Some(&mut address_space_cache),
                    )
                    .unwrap(),
            );
        })
    });
    group.bench_function("read_u32_with_cache", |b| {
        b.iter(|| {
            black_box(
                address_space
                    .read_le_value::<u32>(
                        black_box(0x0000),
                        machine.now(),
                        Some(&mut address_space_cache),
                    )
                    .unwrap(),
            );
        })
    });
    group.bench_function("read_u64_with_cache", |b| {
        b.iter(|| {
            black_box(
                address_space
                    .read_le_value::<u64>(
                        black_box(0x0000),
                        machine.now(),
                        Some(&mut address_space_cache),
                    )
                    .unwrap(),
            );
        })
    });

    group.bench_function("read_u8_from_rom", |b| {
        b.iter(|| {
            black_box(
                address_space
                    .read_le_value::<u8>(black_box(0x1000), machine.now(), None)
                    .unwrap(),
            );
        })
    });
    group.bench_function("read_u16_from_rom", |b| {
        b.iter(|| {
            black_box(
                address_space
                    .read_le_value::<u16>(black_box(0x1000), machine.now(), None)
                    .unwrap(),
            );
        })
    });
    group.bench_function("read_u32_from_rom", |b| {
        b.iter(|| {
            black_box(
                address_space
                    .read_le_value::<u32>(black_box(0x1000), machine.now(), None)
                    .unwrap(),
            );
        })
    });
    group.bench_function("read_u64_from_rom", |b| {
        b.iter(|| {
            black_box(
                address_space
                    .read_le_value::<u64>(black_box(0x1000), machine.now(), None)
                    .unwrap(),
            );
        })
    });
    group.bench_function("read_u8_from_rom_with_cache", |b| {
        b.iter(|| {
            black_box(
                address_space
                    .read_le_value::<u8>(
                        black_box(0x1000),
                        machine.now(),
                        Some(&mut address_space_cache),
                    )
                    .unwrap(),
            );
        })
    });
    group.bench_function("read_u16_from_rom_with_cache", |b| {
        b.iter(|| {
            black_box(
                address_space
                    .read_le_value::<u16>(
                        black_box(0x1000),
                        machine.now(),
                        Some(&mut address_space_cache),
                    )
                    .unwrap(),
            );
        })
    });
    group.bench_function("read_u32_from_rom_with_cache", |b| {
        b.iter(|| {
            black_box(
                address_space
                    .read_le_value::<u32>(
                        black_box(0x1000),
                        machine.now(),
                        Some(&mut address_space_cache),
                    )
                    .unwrap(),
            );
        })
    });
    group.bench_function("read_u64_from_rom_with_cache", |b| {
        b.iter(|| {
            black_box(
                address_space
                    .read_le_value::<u64>(
                        black_box(0x0000),
                        machine.now(),
                        Some(&mut address_space_cache),
                    )
                    .unwrap(),
            );
        })
    });

    group.bench_function("write_u8", |b| {
        b.iter(|| {
            address_space
                .write_le_value::<u8>(black_box(0x0000), machine.now(), None, black_box(0))
                .unwrap();
        })
    });
    group.bench_function("write_u16", |b| {
        b.iter(|| {
            address_space
                .write_le_value::<u16>(black_box(0x0000), machine.now(), None, black_box(0))
                .unwrap();
        })
    });
    group.bench_function("write_u32", |b| {
        b.iter(|| {
            address_space
                .write_le_value::<u32>(black_box(0x0000), machine.now(), None, black_box(0))
                .unwrap();
        })
    });
    group.bench_function("write_u64", |b| {
        b.iter(|| {
            address_space
                .write_le_value::<u64>(black_box(0x0000), machine.now(), None, black_box(0))
                .unwrap();
        })
    });

    group.bench_function("write_u8_with_cache", |b| {
        b.iter(|| {
            address_space
                .write_le_value::<u8>(
                    black_box(0x0000),
                    machine.now(),
                    Some(&mut address_space_cache),
                    black_box(0),
                )
                .unwrap();
        })
    });
    group.bench_function("write_u16_with_cache", |b| {
        b.iter(|| {
            address_space
                .write_le_value::<u16>(
                    black_box(0x0000),
                    machine.now(),
                    Some(&mut address_space_cache),
                    black_box(0),
                )
                .unwrap();
        })
    });
    group.bench_function("write_u32_with_cache", |b| {
        b.iter(|| {
            address_space
                .write_le_value::<u32>(
                    black_box(0x0000),
                    machine.now(),
                    Some(&mut address_space_cache),
                    black_box(0),
                )
                .unwrap();
        })
    });
    group.bench_function("write_u64_with_cache", |b| {
        b.iter(|| {
            address_space
                .write_le_value::<u64>(
                    black_box(0x0000),
                    machine.now(),
                    Some(&mut address_space_cache),
                    black_box(0),
                )
                .unwrap();
        })
    });
}

criterion_group!(benches, criterion_benchmark);
criterion_main!(benches);
