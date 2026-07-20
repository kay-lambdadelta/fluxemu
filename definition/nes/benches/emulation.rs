use std::time::Duration;

use criterion::{Criterion, Throughput, criterion_group, criterion_main};
use fluxemu_definition_nes::Nes;
use fluxemu_environment::find_and_load_environment;
use fluxemu_program::ProgramManager;
use fluxemu_runtime::machine::{Machine, builder::MachineFactory};
use redb::Database;

fn emulation_performance(c: &mut Criterion) {
    let roms = [
        (
            "Pennant League!! - Home Run Nighter (Japan).nes",
            "30b02626f7432213b828ab245d7b0335d7b5baf5",
        ),
        (
            "Legend of Zelda, The (USA) (Rev 1).nes",
            "4671517d72d09799403f6c672cd2b395933e926e",
        ),
        (
            "Hydlide (USA).nes",
            "f9767c7b90b2909fa1e90ade58153bcb911c107f",
        ),
        (
            "Super Mario Bros. (World).nes",
            "33d23c2f2cfa4c9efec87f7bc1321ce3ce6c89bd",
        ),
        (
            "Pac-Man (USA) (Tengen).nes",
            "aa3d1672d679a5ca8625ebe67ce46d805719d3fe",
        ),
        (
            "1942 (Japan, USA) (En).nes",
            "1fc8410c271441b313ad4b382fbe9dcd9eefb6cb",
        ),
        (
            "Xevious (Japan) (En).nes",
            "c4a36c14de32424f0ee2f9cd3fa8ec28103f9bf0",
        ),
    ];

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
            let machine = Nes.construct(machine).seal().build(());

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
