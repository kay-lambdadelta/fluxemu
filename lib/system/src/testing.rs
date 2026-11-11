extern crate std;

use std::{
    eprintln, format, io::Write, ops::RangeInclusive, prelude::rust_2024::*, sync::Arc,
    time::Duration,
};

use criterion::{Criterion, Throughput};
use fluxemu_database::ProgramDatabase;
use fluxemu_environment::load_environment;
use fluxemu_input::{InputId, InputState};
use fluxemu_math::range::ContiguousRange;
use fluxemu_program::{Manifest, ProgramManager};
use fluxemu_runtime::{
    machine::{Machine, RuntimeGuard},
    memory::{Address, AddressSpaceId},
    platform::TestPlatform,
    scheduler::Period,
};
use redb::ReadOnlyDatabase;
use tokio::runtime::Runtime;

use crate::System;

const WARMUP_FRAMES: u32 = 240;
const CONTINUATION_FRAMES: u32 = 90;
const INPUT_INTERVAL_FRAMES: u32 = 30;
const INPUT_HOLD_FRAMES: u32 = 6;
const FRAME_FRACTIONS: [u32; 2] = [7, 11];
const BLARGG_SIGNATURE: [u8; 3] = [0xde, 0xb0, 0x61];
const BLARGG_RUNNING: u8 = 0x80;
const BLARGG_TEXT_LIMIT: usize = 0x1000;

#[derive(Debug, Clone, Copy)]
pub enum TestRom {
    Stored {
        name: &'static str,
        /// String literal of a RomId, which is a BLAKE3 hash
        id: &'static str,
    },
    Embedded {
        file_name: &'static str,
        data: &'static [u8],
    },
}

impl TestRom {
    fn name(&self) -> &'static str {
        match self {
            TestRom::Stored { name, .. } => name,
            TestRom::Embedded { file_name, .. } => file_name,
        }
    }
}

struct Context {
    runtime: Runtime,
    program_database: Option<Arc<ProgramDatabase<ReadOnlyDatabase>>>,
    program_manager: Arc<ProgramManager>,
}

impl Context {
    fn new() -> Self {
        let environment = load_environment();

        let program_database = if environment.database_location.exists() {
            match ReadOnlyDatabase::open(&environment.database_location)
                .map_err(|err| err.to_string())
                .and_then(|database| {
                    ProgramDatabase::read_only(database).map_err(|err| err.to_string())
                }) {
                Ok(database) => Some(database),
                Err(err) => {
                    let _ = writeln!(
                        std::io::stderr(),
                        "warning: not using program database: {err}"
                    );

                    None
                }
            }
        } else {
            None
        };

        Self {
            runtime: Runtime::new().unwrap(),
            program_database,
            program_manager: ProgramManager::new(environment.rom_stores),
        }
    }

    fn resolve(&self, rom: &TestRom) -> Option<Manifest> {
        self.runtime.block_on(async {
            match rom {
                TestRom::Stored { name, id } => {
                    let id = match id.parse() {
                        Ok(id) => id,
                        Err(err) => {
                            return None;
                        }
                    };

                    let digest = self.program_manager.digest(id).await.unwrap()?;

                    let mut specifications = match &self.program_database {
                        Some(program_database) => program_database
                            .identify_program(&[digest])
                            .unwrap_or_default(),
                        None => Vec::new(),
                    };

                    if specifications.is_empty() {
                        self.program_manager
                            .auto_generate_specification(id)
                            .await
                            .unwrap()
                    } else {
                        Some(specifications.remove(0).manifest)
                    }
                }
                TestRom::Embedded { data, .. } => {
                    let id = self.program_manager.register_embedded_rom(data);

                    self.program_manager
                        .auto_generate_specification(id)
                        .await
                        .unwrap()
                }
            }
        })
    }

    fn build<S: System<TestPlatform, Quirks: Default>>(
        &self,
        system: &S,
        specification: &Manifest,
    ) -> Arc<Machine> {
        self.runtime.block_on(async {
            let machine =
                Machine::build_test(Some(specification.clone()), self.program_manager.clone());

            system
                .build(S::Quirks::default(), machine)
                .await
                .unwrap()
                .build(())
        })
    }
}

fn frame() -> Period {
    Period::from_num(1) / Period::from_num(60)
}

fn set_input(machine: &Arc<Machine>, selection: usize, state: InputState) {
    let runtime = machine.enter_runtime();

    let mut device_paths: Vec<_> = runtime.input_devices().keys().cloned().collect();
    device_paths.sort_unstable();

    for path in device_paths {
        let mut inputs: Vec<InputId> = runtime.input_devices()[&path]
            .metadata()
            .present_inputs
            .iter()
            .copied()
            .collect();
        inputs.sort_unstable();

        if !inputs.is_empty() {
            runtime.insert_inputs(&path, [(inputs[selection % inputs.len()], state)]);
        }
    }
}

fn drive(machine: &Arc<Machine>, frames: u32) {
    for index in 0..frames {
        let selection = (index / INPUT_INTERVAL_FRAMES) as usize;

        match index % INPUT_INTERVAL_FRAMES {
            0 => set_input(machine, selection, InputState::PRESSED),
            INPUT_HOLD_FRAMES => set_input(machine, selection, InputState::RELEASED),
            _ => {}
        }

        machine.enter_runtime().run(frame());
    }
}

struct Capture {
    snapshot: Vec<u8>,
    memory_map_hash: [u8; 32],
}

impl Capture {
    fn assert_matches(&self, expected: &Capture, name: &str, stage: &str) {
        assert!(
            self.snapshot == expected.snapshot,
            "{name}: {stage} snapshot differs"
        );
        assert!(
            self.memory_map_hash == expected.memory_map_hash,
            "{name}: {stage} memory map differs"
        );
    }
}

fn capture(machine: &Arc<Machine>) -> Capture {
    let guard = machine.enter_runtime_exclusive();

    let mut snapshot = Vec::new();
    guard.store_snapshot(&mut snapshot).unwrap();

    Capture {
        snapshot,
        memory_map_hash: guard.memory_map_hash(),
    }
}

fn load(machine: &Arc<Machine>, snapshot: &[u8]) {
    machine
        .enter_runtime_exclusive()
        .load_snapshot(snapshot, true)
        .unwrap();
}

fn store_save(machine: &Arc<Machine>) -> Option<Vec<u8>> {
    let guard = machine.enter_runtime_exclusive();

    guard.has_save_data().then(|| {
        let mut output = Vec::new();
        guard.store_save(&mut output, None).unwrap();

        output
    })
}

fn check_save<S: System<TestPlatform, Quirks: Default>>(
    context: &Context,
    system: &S,
    specification: &Manifest,
    machine: &Arc<Machine>,
    name: &str,
) {
    let Some(save) = store_save(machine) else {
        return;
    };

    let fresh = context.build(system, specification);

    fresh
        .enter_runtime_exclusive()
        .load_save(&save, true)
        .unwrap();

    assert!(
        store_save(&fresh).as_ref() == Some(&save),
        "{name}: save differs after loading"
    );
}

fn check_rom<S: System<TestPlatform, Quirks: Default>>(
    context: &Context,
    system: &S,
    rom: &TestRom,
) {
    let name = rom.name();

    let Some(specification) = context.resolve(rom) else {
        let _ = writeln!(
            std::io::stderr(),
            "warning: skipping snapshot test for missing program {name}"
        );

        return;
    };

    for fraction in FRAME_FRACTIONS {
        let machine = context.build(system, &specification);

        drive(&machine, WARMUP_FRAMES);
        machine
            .enter_runtime()
            .run(frame() / Period::from_num(fraction));

        check_save(context, system, &specification, &machine, name);

        let snapshot = capture(&machine);

        drive(&machine, CONTINUATION_FRAMES);
        let expected = capture(&machine);

        assert!(
            snapshot.snapshot != expected.snapshot,
            "{name}: machine state never changed"
        );

        load(&machine, &snapshot.snapshot);
        capture(&machine).assert_matches(&snapshot, name, "restore");

        drive(&machine, CONTINUATION_FRAMES);
        capture(&machine).assert_matches(&expected, name, "replay");

        let fresh = context.build(system, &specification);

        load(&fresh, &snapshot.snapshot);
        capture(&fresh).assert_matches(&snapshot, name, "fresh restore");

        drive(&fresh, CONTINUATION_FRAMES);
        capture(&fresh).assert_matches(&expected, name, "fresh replay");
    }
}

pub fn assert_snapshots_reproducible<S: System<TestPlatform, Quirks: Default>>(
    system: S,
    roms: &[TestRom],
) {
    let context = Context::new();

    std::thread::scope(|scope| {
        for rom in roms {
            let context = &context;
            let system = &system;

            scope.spawn(move || check_rom(context, system, rom));
        }
    });
}

pub fn benchmark_emulation<S: System<TestPlatform, Quirks: Default>>(
    criterion: &mut Criterion,
    package: &str,
    system: S,
    roms: &[TestRom],
) {
    let context = Context::new();

    let mut group = criterion.benchmark_group(format!("{package}/emulation_performance"));
    group.throughput(Throughput::Elements(1));

    for rom in roms {
        let name = rom.name();

        let Some(specification) = context.resolve(rom) else {
            eprintln!("Skipping benchmark for missing program {name}");

            continue;
        };

        let machine = context.build(&system, &specification);

        group.bench_function(name, |bencher| {
            bencher.iter(|| {
                let runtime_guard = machine.enter_runtime();

                runtime_guard.run_duration(Duration::from_secs(1));
            });
        });
    }

    group.finish();
}

pub fn run_until<S: System<TestPlatform, Quirks: Default>, T>(
    system: S,
    rom: &TestRom,
    frame_limit: u32,
    mut poll: impl FnMut(&RuntimeGuard) -> Option<T>,
) -> Option<T> {
    let context = Context::new();
    let name = rom.name();

    let specification = context
        .resolve(rom)
        .unwrap_or_else(|| std::panic!("{name}: missing test program"));
    let machine = context.build(&system, &specification);

    for _ in 0..frame_limit {
        let runtime = machine.enter_runtime();
        runtime.run(frame());

        if let Some(result) = poll(&runtime) {
            return Some(result);
        }
    }

    None
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlarggStatus {
    pub code: u8,
    pub text: String,
}

impl BlarggStatus {
    pub fn is_running(&self) -> bool {
        self.code >= BLARGG_RUNNING
    }
}

// For further information visit:
//
// /external/nes-test-roms/instr_test-v5/readme.txt
pub fn read_blargg_status(
    runtime: &RuntimeGuard,
    address_space: AddressSpaceId,
    base: Address,
) -> Option<BlarggStatus> {
    let mut address_space = runtime.address_space(address_space).unwrap();
    let timestamp = runtime.safe_advance_timestamp();

    let mut read = |address| {
        address_space
            .read_le_value::<u8, true>(address, timestamp)
            .ok()
    };

    for (offset, expected) in BLARGG_SIGNATURE.into_iter().enumerate() {
        if read(base + 1 + offset)? != expected {
            return None;
        }
    }

    let code = read(base)?;

    let text = RangeInclusive::from_start_and_length(base + 4, BLARGG_TEXT_LIMIT)
        .map_while(|address| read(address).filter(|&byte| byte != 0))
        .map(char::from)
        .collect();

    Some(BlarggStatus { code, text })
}
