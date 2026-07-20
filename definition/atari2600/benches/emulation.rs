use std::time::Duration;

use criterion::{Criterion, Throughput, criterion_group, criterion_main};
use fluxemu_definition_atari2600::Atari2600;
use fluxemu_environment::find_and_load_environment;
use fluxemu_program::ProgramManager;
use fluxemu_runtime::machine::{Machine, builder::MachineFactory};
use redb::Database;

fn emulation_performance(c: &mut Criterion) {
    let roms = [(
        "Donkey Kong (USA).a26",
        "6e6e37ec8d66aea1c13ed444863e3db91497aa35",
    )];

    let (_, environment) = find_and_load_environment();

    let database = Database::create(environment.database_location).unwrap();
    let program_manager =
        ProgramManager::new(database, environment.rom_store_directories.clone()).unwrap();

    let mut group = c.benchmark_group(format!("{}/emulation_performance", env!("CARGO_PKG_NAME")));
    group.throughput(Throughput::Elements(1));

    for (program_name, rom_id) in roms {
        let rom_id = rom_id.parse().unwrap();

        if let Some(specification) = program_manager
            .identify_program(&[rom_id])
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
            let machine = Atari2600.construct(machine).seal().build(());

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
