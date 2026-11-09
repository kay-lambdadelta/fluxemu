use std::{
    fs::File,
    io::{Error, ErrorKind, Write},
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    thread::JoinHandle,
    time::Duration,
};

use fluxemu_program::ProgramId;
use fluxemu_runtime::machine::Machine;
use serde::{Deserialize, Serialize};

const ACTIVE_SLOT_FILE: &str = "active.toml";

pub(crate) fn program_directory(root: &Path, id: &ProgramId) -> PathBuf {
    root.join(id.path())
}

#[derive(Debug, Serialize, Deserialize)]
struct ActiveSlot {
    slot: u8,
}

pub(crate) fn active_save_slot(directory: &Path) -> u8 {
    std::fs::read_to_string(directory.join(ACTIVE_SLOT_FILE))
        .ok()
        .and_then(|contents| toml::from_str::<ActiveSlot>(&contents).ok())
        .map_or(0, |active| active.slot)
}

pub(crate) fn set_active_save_slot(directory: &Path, slot: u8) -> std::io::Result<()> {
    let contents = toml::to_string(&ActiveSlot { slot })
        .map_err(|error| Error::new(ErrorKind::InvalidData, error))?;

    write_atomically(&directory.join(ACTIVE_SLOT_FILE), contents.as_bytes())
}

pub(crate) fn save_path(root: &Path, id: &ProgramId) -> PathBuf {
    let directory = program_directory(root, id);
    let slot = active_save_slot(&directory);

    directory.join(format!("{slot}.save"))
}

pub(crate) fn write_atomically(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    static NEXT_TEMPORARY_FILE: AtomicUsize = AtomicUsize::new(0);

    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let temporary_path = path.with_extension(format!(
        "{}.tmp",
        NEXT_TEMPORARY_FILE.fetch_add(1, Ordering::Relaxed)
    ));

    let result = File::create(&temporary_path)
        .and_then(|mut file| {
            file.write_all(bytes)?;
            file.sync_all()
        })
        .and_then(|()| std::fs::rename(&temporary_path, path));

    if result.is_err() {
        let _ = std::fs::remove_file(&temporary_path);
    }

    result
}

pub(crate) fn open_save(machine: &Arc<Machine>, path: &Path, check_hash: bool) -> Option<Vec<u8>> {
    let guard = machine.enter_runtime_exclusive();

    if !guard.has_save_data() {
        return None;
    }

    let existing = match std::fs::read(path) {
        Ok(bytes) => Some(bytes),
        Err(error) if error.kind() == ErrorKind::NotFound => None,
        Err(error) => {
            tracing::error!("Could not read save {}: {}", path.display(), error);

            return None;
        }
    };

    if let Some(existing) = &existing
        && let Err(error) = guard.load_save(existing, check_hash)
    {
        tracing::error!(
            "Could not load save {}, it will not be overwritten: {}",
            path.display(),
            error
        );

        return None;
    }

    let mut baseline = Vec::new();

    if let Err(error) = guard.store_save(&mut baseline, existing.as_deref()) {
        tracing::error!("Could not capture save: {}", error);

        return None;
    }

    Some(baseline)
}

fn flush(machine: &Arc<Machine>, path: &Path, last_written: &mut Vec<u8>) {
    let mut next = Vec::new();

    if let Err(error) = machine
        .enter_runtime_exclusive()
        .store_save(&mut next, Some(last_written))
    {
        tracing::error!("Could not capture save: {}", error);

        return;
    }

    if next == *last_written {
        return;
    }

    match write_atomically(path, &next) {
        Ok(()) => *last_written = next,
        Err(error) => tracing::error!("Could not write save {}: {}", path.display(), error),
    }
}

#[derive(Debug)]
pub(crate) struct SaveFlusher {
    should_exit: Arc<AtomicBool>,
    handle: Option<JoinHandle<()>>,
}

impl SaveFlusher {
    pub fn new(
        machine: Arc<Machine>,
        path: PathBuf,
        interval: Duration,
        mut last_written: Vec<u8>,
    ) -> Self {
        let should_exit = Arc::new(AtomicBool::new(false));

        let handle = std::thread::Builder::new()
            .name("save_flusher".to_string())
            .spawn({
                let should_exit = should_exit.clone();

                move || {
                    while !should_exit.load(Ordering::Acquire) {
                        if interval.is_zero() {
                            std::thread::park();
                        } else {
                            std::thread::park_timeout(interval);
                        }

                        if !interval.is_zero() && !should_exit.load(Ordering::Acquire) {
                            flush(&machine, &path, &mut last_written);
                        }
                    }

                    flush(&machine, &path, &mut last_written);
                }
            })
            .expect("Failed to spawn save flusher thread");

        Self {
            should_exit,
            handle: Some(handle),
        }
    }
}

impl Drop for SaveFlusher {
    fn drop(&mut self) {
        self.should_exit.store(true, Ordering::Release);

        let handle = self.handle.take().unwrap();
        handle.thread().unpark();

        handle.join().unwrap();
    }
}
