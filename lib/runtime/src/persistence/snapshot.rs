use fluxemu_graphics::api::GraphicsApi;
use fluxemu_program::{ProgramInfo, RomId};
use itertools::Itertools;
use sdd::Guard;
use serde::{Deserialize, Serialize};

use crate::{
    ResourcePath, RuntimeHandle,
    machine::{ExclusiveRuntimeGuard, Machine},
    persistence::{
        ArchiveKind, SnapshotError,
        archive::{ArchiveReader, ArchiveWriter, EntryKey, Identity},
    },
    scheduler::{
        Period,
        queue::{DecodedQueue, QueueSnapshot},
    },
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct TaskTiming {
    pub current_timestamp: Period,
    pub required_next_allocation: Period,
    pub last_requeued_deadline: Period,
}

#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct ComponentRecord {
    pub section_version: u32,
    pub tasks: Vec<(String, TaskTiming)>,
}

impl ComponentRecord {
    pub fn parse(mut bytes: &[u8]) -> Result<(Self, &[u8]), SnapshotError> {
        let record = rmp_serde::from_read(&mut bytes)?;

        Ok((record, bytes))
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct SchedulerRecord {
    safe_advance_timestamp: Period,
    queue: QueueSnapshot,
}

#[derive(Debug)]
struct PreparedSnapshot<'a> {
    reader: ArchiveReader<'a>,
    safe_advance_timestamp: Period,
    queue: DecodedQueue,
}

pub(super) fn machine_identity(machine: &Machine) -> Identity {
    let specification = machine.program_specification();

    Identity {
        program: specification.map(|specification| specification.id.clone()),
        roms: specification
            .map(|specification| rom_ids(&specification.info))
            .unwrap_or_default(),
    }
}

fn rom_ids(info: &ProgramInfo) -> Vec<RomId> {
    info.filesystem().keys().copied().collect()
}

impl ExclusiveRuntimeGuard<'_> {
    pub fn store_snapshot(&self, output: &mut Vec<u8>) -> Result<(), SnapshotError> {
        let runtime = self.runtime();
        let machine = runtime.machine();
        let registry = runtime.component_registry();
        let memory = runtime.memory_registry();

        let mut writer =
            ArchiveWriter::new(output, ArchiveKind::Snapshot, &machine_identity(machine))?;

        writer.entry(EntryKey::Scheduler, |output| {
            let record = SchedulerRecord {
                safe_advance_timestamp: machine.scheduler.safe_advance_timestamp(),
                queue: machine.scheduler.queue.capture(&registry)?,
            };

            rmp_serde::encode::write(output, &record)?;

            Ok(())
        })?;

        for path in registry.sorted_paths() {
            writer.entry(EntryKey::Component(path.clone()), |output| {
                registry.store_component(path, output)
            })?;
        }

        for path in memory.sorted_paths() {
            writer.entry(EntryKey::Memory(path.clone()), |output| {
                memory.read_region(path, output);

                Ok(())
            })?;
        }

        writer.finish();

        Ok(())
    }

    pub fn read_primary_framebuffer<G: GraphicsApi, T>(
        &self,
        callback: impl FnOnce(&G::Framebuffer) -> T,
    ) -> Option<T> {
        let framebuffers = &self.runtime().machine().framebuffers;

        let framebuffer = framebuffers
            .iter()
            .min_by_key(|(path, _)| *path)?
            .1
            .lock()
            .unwrap();

        Some(callback((*framebuffer).downcast_ref()?))
    }

    pub fn save_regions(&self) -> Vec<ResourcePath> {
        self.runtime()
            .memory_registry()
            .sorted_save_paths()
            .into_iter()
            .cloned()
            .collect()
    }

    pub fn memory_map_hash(&self) -> [u8; blake3::OUT_LEN] {
        let mut hasher = blake3::Hasher::new();

        for address_space in self
            .runtime()
            .machine()
            .address_spaces
            .values()
            .sorted_by_key(|space| space.id())
        {
            address_space.hash_memory_map(&mut hasher);
        }

        *hasher.finalize().as_bytes()
    }

    pub fn load_snapshot(&self, bytes: &[u8], check_hash: bool) -> Result<(), SnapshotError> {
        let prepared = self.prepare(bytes, check_hash)?;

        let mut undo = Vec::new();
        self.store_snapshot(&mut undo)?;

        if let Err(error) = self.apply(prepared) {
            let prepared = self
                .prepare(&undo, false)
                .expect("Undo snapshot must be valid");

            self.apply(prepared).expect("Undo snapshot must apply");

            return Err(error);
        }

        Ok(())
    }

    fn prepare<'a>(
        &self,
        bytes: &'a [u8],
        check_hash: bool,
    ) -> Result<PreparedSnapshot<'a>, SnapshotError> {
        let runtime = self.runtime();
        let machine = runtime.machine();
        let registry = runtime.component_registry();
        let memory = runtime.memory_registry();

        let reader = ArchiveReader::open(bytes, ArchiveKind::Snapshot, check_hash)?;

        if *reader.identity() != machine_identity(machine) {
            return Err(SnapshotError::WrongProgram);
        }

        let component_paths = registry.sorted_paths();
        let memory_paths = memory.sorted_paths();

        if reader.entry_count() != 1 + component_paths.len() + memory_paths.len() {
            return Err(SnapshotError::LayoutMismatch);
        }

        for path in component_paths {
            let record = reader
                .entry(&EntryKey::Component(path.clone()))
                .ok_or_else(|| SnapshotError::MissingComponent(path.clone()))?;

            registry.validate_component(path, record)?;
        }

        for path in memory_paths {
            let region = reader
                .entry(&EntryKey::Memory(path.clone()))
                .ok_or_else(|| SnapshotError::MissingMemory(path.clone()))?;

            let expected = memory.region_size(path).unwrap();

            if region.len() != expected {
                return Err(SnapshotError::RegionSizeMismatch {
                    path: path.clone(),
                    expected,
                    found: region.len(),
                });
            }
        }

        let scheduler = reader
            .entry(&EntryKey::Scheduler)
            .ok_or(SnapshotError::MissingScheduler)?;
        let scheduler: SchedulerRecord = rmp_serde::from_slice(scheduler)?;

        Ok(PreparedSnapshot {
            safe_advance_timestamp: scheduler.safe_advance_timestamp,
            queue: scheduler.queue.decode(&registry)?,
            reader,
        })
    }

    fn apply(&self, prepared: PreparedSnapshot<'_>) -> Result<(), SnapshotError> {
        let runtime = self.runtime();
        let machine = runtime.machine();
        let registry = runtime.component_registry();
        let memory = runtime.memory_registry();

        let PreparedSnapshot {
            reader,
            safe_advance_timestamp,
            queue,
        } = prepared;

        let component_paths = registry.sorted_paths();

        for path in &component_paths {
            let record = reader.entry(&EntryKey::Component((*path).clone())).unwrap();

            registry.load_component_state(path, record)?;
        }

        for path in memory.sorted_paths() {
            memory.write_region(path, reader.entry(&EntryKey::Memory(path.clone())).unwrap());
        }

        machine
            .scheduler
            .set_safe_advance_timestamp(safe_advance_timestamp);

        restore_memory_maps(runtime);

        for path in component_paths {
            registry
                .interact_dyn(path, Period::ZERO, |component| component.state_restored())
                .unwrap();
        }

        machine.scheduler.queue.restore(queue);

        Ok(())
    }
}

fn restore_memory_maps(runtime: &RuntimeHandle) {
    let guard = Guard::new();

    for address_space in runtime
        .machine()
        .address_spaces
        .values()
        .sorted_by_key(|space| space.id())
    {
        address_space.restore_baseline(&guard, runtime);
    }
}
