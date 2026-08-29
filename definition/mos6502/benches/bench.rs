use std::{ops::RangeInclusive, sync::Arc, time::Duration};

use bytes::Bytes;
use criterion::{Criterion, Throughput, criterion_group, criterion_main};
use fluxemu_definition_mos6502::{
    Config,
    variant::{Mos6502, Mos6507, Ricoh2A0x, Variant, Wdc65C02},
};
use fluxemu_math::range::ContiguousRange;
use fluxemu_runtime::{
    machine::Machine,
    memory::{MapTarget, MemoryMapCommand, Permissions},
    scheduler::Frequency,
};

fn produce_memory() -> [u8; 0x10000] {
    let mut memory = [0u8; 0x10000];

    // Reset vector
    memory[0xfffc] = 0x00;
    memory[0xfffd] = 0x02;

    // Program
    #[rustfmt::skip]
    let code = [
        // lda #$00
        0xa9, 0x00,
        // sta $10
        0x85, 0x10,
        // lda $10
        0xa5, 0x10,
        // clc
        0x18,
        // adc #$01
        0x69, 0x01,
        // sta $10
        0x85, 0x10,
        // inc $11
        0xe6, 0x11,
        // ldx $11
        0xa6, 0x11,
        // cpx #$ff
        0xe0, 0xff,
        // bne $0204
        0xd0, 0xf1,
        // jmp $0200
        0x4c, 0x00, 0x02,
    ];

    memory[RangeInclusive::from_start_and_length(0x0200, code.len())].copy_from_slice(&code);

    memory
}

fn build_machine<V: Variant>() -> Arc<Machine> {
    let (machine, address_space) = Machine::build_test_minimal().address_space(16);

    let (machine, ram_path) = machine.memory(
        "ram",
        0x10000,
        [(
            RangeInclusive::from_start_and_length(0, 0x10000),
            Bytes::from_owner(produce_memory()),
        )],
    );

    let machine = machine.map_memory(
        address_space,
        [MemoryMapCommand::Map {
            range: RangeInclusive::from_start_and_length(0, 0x10000),
            permissions: Permissions::ALL,
            target: MapTarget::Memory {
                path: ram_path,
                subrange: None,
            },
        }],
    );

    let (machine, _) = machine.component(
        "cpu",
        Config::<V>::new(Frequency::from_num(1000000), address_space),
    );

    machine.seal().build(())
}

fn bench_variant<V: fluxemu_definition_mos6502::variant::Variant>(c: &mut Criterion, name: &str) {
    let machine = build_machine::<V>();
    let runtime_guard = machine.enter_runtime();

    let mut group = c.benchmark_group(format!("{}/mos6502/{}", env!("CARGO_PKG_NAME"), name));
    group.throughput(Throughput::Elements(1000000));

    group.bench_function("1 MHZ", |b| {
        b.iter(|| runtime_guard.run_duration(Duration::from_secs(1)));
    });

    group.finish();
}

fn bench(c: &mut Criterion) {
    bench_variant::<Mos6502>(c, "mos6502");
    bench_variant::<Mos6507>(c, "mos6507");
    bench_variant::<Ricoh2A0x>(c, "ricoh2a0x");
    bench_variant::<Wdc65C02>(c, "wdc65c02");
}

criterion_group!(benches, bench);
criterion_main!(benches);
