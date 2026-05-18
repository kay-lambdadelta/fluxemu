use std::{ops::Deref, time::Duration};

use criterion::{Criterion, Throughput, criterion_group, criterion_main};
use fluxemu_definition_nes::Nes;
use fluxemu_environment::{ENVIRONMENT_LOCATION, Environment};
use fluxemu_program::ProgramManager;
use fluxemu_runtime::machine::{Machine, builder::MachineFactory};
use redb::Database;

fn emulation_performance(c: &mut Criterion) {
    // All from https://github.com/christopherpow/nes-test-roms
    let rom_ids = [
        // spritecans-2011
        "3482c7c5feb5c75406b18cacbd2fd3e96b16b3a5",
        // stars_se
        "ba1f0ba0f8b8f0c43cfa1659f86e5dd75a48a1fe",
    ];

    let environment = if let Ok(environment_string) =
        std::fs::read_to_string(ENVIRONMENT_LOCATION.deref())
        && let Ok(environment) = ron::from_str(&environment_string)
    {
        environment
    } else {
        Environment::default()
    };

    let database = Database::create(environment.database_location).unwrap();
    let program_manager = ProgramManager::new(database, [environment.rom_store]).unwrap();

    let mut group = c.benchmark_group(format!("{}/emulation_performance", env!("CARGO_PKG_NAME")));
    group.throughput(Throughput::Elements(1));

    for rom_id in rom_ids {
        let rom_id = rom_id.parse().unwrap();

        let machine = Machine::build_test(
            Some(
                program_manager
                    .auto_generate_specification(rom_id)
                    .unwrap()
                    .unwrap(),
            ),
            program_manager.clone(),
        );

        let machine = Nes.construct(machine).seal().build(());

        group.bench_function(rom_id.to_string(), |b| {
            b.iter(|| {
                let runtime_guard = machine.enter_runtime();

                runtime_guard.run_duration(Duration::from_secs(1));
            });
        });
    }

    group.finish();
}

criterion_group!(benches, emulation_performance);
criterion_main!(benches);
