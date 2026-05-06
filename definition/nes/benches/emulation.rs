use std::{ops::Deref, time::Duration};

use divan::Bencher;
use fluxemu_definition_nes::Nes;
use fluxemu_environment::{ENVIRONMENT_LOCATION, Environment};
use fluxemu_program::ProgramManager;
use fluxemu_runtime::machine::{Machine, builder::MachineFactory};
use redb::Database;

fn main() {
    divan::main();
}

// All from https://github.com/christopherpow/nes-test-roms

#[divan::bench(args = [
    // https://github.com/christopherpow/nes-test-roms/tree/master/spritecans-2011
    "3482c7c5feb5c75406b18cacbd2fd3e96b16b3a5",
    // https://github.com/christopherpow/nes-test-roms/tree/master/stars_se
    "ba1f0ba0f8b8f0c43cfa1659f86e5dd75a48a1fe",
])]
fn emulation_performance(bencher: Bencher, rom_id: &str) {
    let rom_id = rom_id.parse().unwrap();

    let environment = if let Ok(environment_string) =
        std::fs::read_to_string(ENVIRONMENT_LOCATION.deref())
        && let Ok(environment) = ron::from_str(&environment_string)
    {
        environment
    } else {
        Environment::default()
    };

    let database = Database::open(environment.database_location).unwrap();
    let program_manager = ProgramManager::new(database, [environment.rom_store]).unwrap();

    let machine = Machine::build_test(
        Some(
            program_manager
                .auto_generate_specification(rom_id)
                .unwrap()
                .unwrap(),
        ),
        program_manager,
    );

    let machine = Nes.construct(machine).seal().build(());

    bencher.bench_local(|| {
        let runtime_guard = machine.enter_runtime();
        runtime_guard.run_duration(Duration::from_secs(1));
    });
}
