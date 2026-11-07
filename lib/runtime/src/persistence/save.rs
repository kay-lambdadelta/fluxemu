use crate::{
    machine::ExclusiveRuntimeGuard,
    persistence::{
        ArchiveKind, SnapshotError,
        archive::{ArchiveReader, ArchiveWriter, EntryKey},
        snapshot::machine_identity,
    },
    scheduler::Period,
};

impl ExclusiveRuntimeGuard<'_> {
    pub fn has_save_data(&self) -> bool {
        let registry = self.runtime().component_registry();

        !self
            .runtime()
            .memory_registry()
            .sorted_save_paths()
            .is_empty()
            || registry
                .sorted_paths()
                .into_iter()
                .any(|path| registry.has_save(path))
    }

    pub fn store_save(
        &self,
        output: &mut Vec<u8>,
        previous: Option<&[u8]>,
    ) -> Result<(), SnapshotError> {
        let runtime = self.runtime();
        let registry = runtime.component_registry();
        let memory = runtime.memory_registry();

        let mut writer = ArchiveWriter::new(
            output,
            ArchiveKind::Save,
            &machine_identity(runtime.machine()),
        )?;

        for path in registry.sorted_paths() {
            if registry.has_save(path) {
                writer.entry(EntryKey::Component(path.clone()), |output| {
                    registry.store_component_save(path, output)
                })?;
            }
        }

        let save_paths = memory.sorted_save_paths();

        for path in &save_paths {
            writer.entry(EntryKey::Memory((*path).clone()), |output| {
                memory.read_region(path, output);

                Ok(())
            })?;
        }

        if let Some(previous) = previous {
            let previous = ArchiveReader::open(previous, ArchiveKind::Save, false)?;

            for (key, bytes) in previous.entries() {
                let known = match key {
                    EntryKey::Component(path) => registry.has_save(path),
                    EntryKey::Memory(path) => save_paths.contains(&path),
                    EntryKey::Scheduler => true,
                };

                if !known {
                    writer.entry(key.clone(), |output| {
                        output.extend_from_slice(bytes);

                        Ok(())
                    })?;
                }
            }
        }

        writer.finish();

        Ok(())
    }

    pub fn load_save(&self, bytes: &[u8], check_hash: bool) -> Result<(), SnapshotError> {
        let runtime = self.runtime();
        let memory = runtime.memory_registry();

        let reader = ArchiveReader::open(bytes, ArchiveKind::Save, check_hash)?;

        if reader.identity().program != machine_identity(runtime.machine()).program {
            return Err(SnapshotError::WrongProgram);
        }

        for path in memory.sorted_save_paths() {
            if let Some(region) = reader.entry(&EntryKey::Memory(path.clone())) {
                let expected = memory.region_size(path).unwrap();

                if region.len() != expected {
                    return Err(SnapshotError::RegionSizeMismatch {
                        path: path.clone(),
                        expected,
                        found: region.len(),
                    });
                }
            }
        }

        let mut undo = Vec::new();
        self.store_snapshot(&mut undo)?;

        if let Err(error) = self.apply_save(&reader) {
            self.load_snapshot(&undo, false)
                .expect("Undo snapshot must apply");

            return Err(error);
        }

        Ok(())
    }

    fn apply_save(&self, reader: &ArchiveReader<'_>) -> Result<(), SnapshotError> {
        let runtime = self.runtime();
        let registry = runtime.component_registry();
        let memory = runtime.memory_registry();

        let mut loaded = Vec::new();

        for path in registry.sorted_paths() {
            if registry.has_save(path)
                && let Some(record) = reader.entry(&EntryKey::Component(path.clone()))
            {
                registry.load_component_save(path, record)?;
                loaded.push(path);
            }
        }

        for path in memory.sorted_save_paths() {
            if let Some(region) = reader.entry(&EntryKey::Memory(path.clone())) {
                memory.write_region(path, region);
            }
        }

        for path in loaded {
            registry
                .interact_dyn(path, Period::ZERO, |component| component.state_restored())
                .unwrap();
        }

        Ok(())
    }
}
