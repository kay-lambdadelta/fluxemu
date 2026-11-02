use std::time::Duration;

use criterion::{Criterion, Throughput, criterion_group, criterion_main};
use fluxemu_database::ProgramDatabase;
use fluxemu_environment::load_environment;
use fluxemu_program::ProgramManager;
use fluxemu_runtime::machine::Machine;
use fluxemu_system::System;
use fluxemu_system_atari_2600::Atari2600;
use redb::Database;

fn emulation_performance(c: &mut Criterion) {
    let roms = [(
        "Donkey Kong (USA).a26",
        "6e6e37ec8d66aea1c13ed444863e3db91497aa35",
    )];

    let environment = load_environment();

    let program_database =
        ProgramDatabase::new(Database::create(environment.database_location).unwrap()).unwrap();
    let program_manager = ProgramManager::new(environment.rom_store_directories.clone());

    let mut group = c.benchmark_group(format!("{}/emulation_performance", env!("CARGO_PKG_NAME")));
    group.throughput(Throughput::Elements(1));

    for (program_name, rom_id) in roms {
        let rom_id = rom_id.parse().unwrap();

        if let Some(specification) = program_database
            .identify_program([rom_id])
            .map(|mut entries| {
                if entries.is_empty() {
                    program_manager.auto_generate_specification(rom_id).unwrap()
                } else {
                    Some(entries.remove(0))
                }
            })
            .unwrap()
            && program_manager.load(rom_id).unwrap().is_some()
        {
            let machine = Machine::build_test(Some(specification), program_manager.clone());
            let machine = Atari2600.build((), machine).unwrap().build(());

            group.bench_function(program_name, |b| {
                b.iter(|| {
                    let runtime_guard = machine.enter_runtime();

                    runtime_guard.run_duration(Duration::from_secs(1));
                });
            });
        } else {
            eprintln!(
                "Skipping benchmark for missing program {} ({})",
                program_name, rom_id
            );
        }
    }

    group.finish();
}

criterion_group!(benches, emulation_performance);
criterion_main!(benches);
