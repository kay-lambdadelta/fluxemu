use std::{
    io::ErrorKind,
    path::{Path, PathBuf},
    sync::Arc,
};

use fluxemu_graphics::texture::OwnedTexture;
use fluxemu_runtime::machine::{ExclusiveRuntimeGuard, Machine};
use image::{ExtendedColorType, ImageFormat, codecs::webp::WebPEncoder};
use palette::Srgba;
use tokio::{runtime::Handle, task::JoinHandle};

use crate::{audio::mixer::AudioMixer, core::release_all_inputs, save::write_atomically};

const UNDO_LOAD_FILE: &str = "undo-load.snapshot";
const UNDO_STORE_FILE: &str = "undo-save.snapshot";
const UNDO_STORE_SLOT_FILE: &str = "undo-save.slot";

pub(crate) fn snapshot_path(directory: &Path, slot: u8) -> PathBuf {
    directory.join(format!("{slot}.snapshot"))
}

pub(crate) fn undo_load_path(directory: &Path) -> PathBuf {
    directory.join(UNDO_LOAD_FILE)
}

fn encode_screenshot(screenshot: &OwnedTexture<Srgba<u8>>) -> Option<Vec<u8>> {
    let mut output = Vec::new();

    WebPEncoder::new_lossless(&mut output)
        .encode(
            bytemuck::cast_slice(screenshot.as_slice()?),
            screenshot.width() as u32,
            screenshot.height() as u32,
            ExtendedColorType::Rgba8,
        )
        .inspect_err(|error| tracing::warn!("Could not encode screenshot: {}", error))
        .ok()?;

    Some(output)
}

pub(crate) fn read_screenshot(directory: &Path, slot: u8) -> Option<OwnedTexture<Srgba<u8>>> {
    let bytes = std::fs::read(snapshot_path(directory, slot).with_extension("webp")).ok()?;

    let image = image::load_from_memory_with_format(&bytes, ImageFormat::WebP)
        .ok()?
        .into_rgba8();

    let (width, height) = image.dimensions();
    let pixels = bytemuck::pod_collect_to_vec(image.as_raw()).into_boxed_slice();

    Some(OwnedTexture::from_storage(
        width as usize,
        height as usize,
        pixels,
    ))
}

fn remove_if_present(path: &Path) -> std::io::Result<()> {
    match std::fs::remove_file(path) {
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(()),
        result => result,
    }
}

fn write_screenshot(path: &Path, screenshot: Option<OwnedTexture<Srgba<u8>>>) {
    let encoded = screenshot.as_ref().and_then(encode_screenshot);

    let result = match encoded {
        Some(encoded) => write_atomically(path, &encoded),
        None => remove_if_present(path),
    };

    if let Err(error) = result {
        tracing::warn!("Could not update screenshot {}: {}", path.display(), error);
    }
}

fn back_up_slot(directory: &Path, slot: u8) -> std::io::Result<()> {
    let path = snapshot_path(directory, slot);

    if !path.exists() {
        return Ok(());
    }

    let backup = directory.join(UNDO_STORE_FILE);

    std::fs::rename(&path, &backup)?;

    remove_if_present(&backup.with_extension("webp"))?;

    let image = path.with_extension("webp");
    if image.exists() {
        std::fs::rename(image, backup.with_extension("webp"))?;
    }

    write_atomically(
        &directory.join(UNDO_STORE_SLOT_FILE),
        slot.to_string().as_bytes(),
    )
}

pub(crate) fn store_snapshot(
    machine: &Arc<Machine>,
    directory: PathBuf,
    slot: u8,
    runtime: &Handle,
    capture_screenshot: impl FnOnce(&ExclusiveRuntimeGuard<'_>) -> Option<OwnedTexture<Srgba<u8>>>,
) -> Option<JoinHandle<()>> {
    let mut snapshot = Vec::new();

    let guard = machine.enter_runtime_exclusive();

    if let Err(error) = guard.store_snapshot(&mut snapshot) {
        tracing::error!("Could not capture snapshot: {}", error);

        return None;
    }

    let screenshot = capture_screenshot(&guard);

    drop(guard);

    Some(runtime.spawn_blocking(move || {
        let path = snapshot_path(&directory, slot);

        if let Some(parent) = path.parent()
            && let Err(error) = std::fs::create_dir_all(parent)
        {
            tracing::error!("Could not create {}: {}", parent.display(), error);

            return;
        }

        if let Err(error) = back_up_slot(&directory, slot) {
            tracing::error!("Could not back up slot {}: {}", slot, error);

            return;
        }

        write_screenshot(&path.with_extension("webp"), screenshot);

        if let Err(error) = write_atomically(&path, &snapshot) {
            tracing::error!("Could not write snapshot {}: {}", path.display(), error);

            return;
        }

        tracing::info!("Stored snapshot in slot {}", slot);
    }))
}

pub(crate) fn load_snapshot(
    machine: &Arc<Machine>,
    audio_mixer: &AudioMixer,
    source: &Path,
    directory: &Path,
    check_hash: bool,
    runtime: &Handle,
) -> Option<JoinHandle<()>> {
    let snapshot = match std::fs::read(source) {
        Ok(snapshot) => snapshot,
        Err(error) if error.kind() == ErrorKind::NotFound => {
            tracing::warn!("Nothing to load at {}", source.display());

            return None;
        }
        Err(error) => {
            tracing::error!("Could not read snapshot {}: {}", source.display(), error);

            return None;
        }
    };

    let mut backup = Vec::new();

    let guard = machine.enter_runtime_exclusive();

    if let Err(error) = guard.store_snapshot(&mut backup) {
        tracing::error!("Could not back up the running machine: {}", error);

        return None;
    }

    if let Err(error) = guard.load_snapshot(&snapshot, check_hash) {
        tracing::error!("Could not load snapshot {}: {}", source.display(), error);

        return None;
    }

    drop(guard);

    audio_mixer.clear();
    release_all_inputs(machine);

    let backup_path = undo_load_path(directory);

    Some(runtime.spawn_blocking(move || {
        if let Err(error) = write_atomically(&backup_path, &backup) {
            tracing::error!("Could not write undo snapshot: {}", error);
        }
    }))
}

fn swap_files(first: &Path, second: &Path) -> std::io::Result<()> {
    let temporary = first.with_extension("swap");

    match (first.exists(), second.exists()) {
        (true, true) => {
            std::fs::rename(first, &temporary)?;
            std::fs::rename(second, first)?;
            std::fs::rename(temporary, second)
        }
        (true, false) => std::fs::rename(first, second),
        (false, true) => std::fs::rename(second, first),
        (false, false) => Ok(()),
    }
}

pub(crate) fn can_undo_store(directory: &Path) -> bool {
    directory.join(UNDO_STORE_SLOT_FILE).exists()
}

pub(crate) fn undo_store(directory: &Path) -> std::io::Result<Option<u8>> {
    let slot = match std::fs::read_to_string(directory.join(UNDO_STORE_SLOT_FILE)) {
        Ok(slot) => slot,
        Err(error) if error.kind() == ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error),
    };

    let Ok(slot) = slot.trim().parse() else {
        return Ok(None);
    };

    let path = snapshot_path(directory, slot);
    let backup = directory.join(UNDO_STORE_FILE);

    swap_files(&path, &backup)?;
    swap_files(&path.with_extension("webp"), &backup.with_extension("webp"))?;

    Ok(Some(slot))
}
