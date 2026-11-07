use criterion::{Criterion, criterion_group, criterion_main};
use fluxemu_system::testing::benchmark_emulation;
use fluxemu_system_nintendo_nes::Nes;

#[path = "../test_roms.rs"]
mod test_roms;

fn emulation_performance(c: &mut Criterion) {
    benchmark_emulation(c, env!("CARGO_PKG_NAME"), Nes, test_roms::TEST_ROMS);
}

criterion_group!(benches, emulation_performance);
criterion_main!(benches);
