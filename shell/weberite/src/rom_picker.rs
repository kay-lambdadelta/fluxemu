use std::{cell::RefCell, path::PathBuf, rc::Rc, sync::mpsc::Sender};

use fluxemu_paths::STORAGE_DIRECTORY;
use fluxemu_program::RomId;
use rfd::AsyncFileDialog;
use wasm_bindgen::{JsCast, JsValue, closure::Closure};

use crate::{Application, display::CanvasGraphicsRuntime};

const OPEN_ROM_BUTTON_ID: &str = "open-rom";

pub fn register_open_rom_button<R: CanvasGraphicsRuntime>(
    application: &Rc<RefCell<Application<R>>>,
    picked_roms: Sender<PathBuf>,
) -> Result<(), JsValue> {
    let button = web_sys::window()
        .and_then(|window| window.document())
        .and_then(|document| document.get_element_by_id(OPEN_ROM_BUTTON_ID))
        .ok_or("This page has no open ROM button")?;

    let application = application.clone();
    let listener = Closure::<dyn FnMut()>::own_aborting(move || {
        application
            .borrow()
            .local_set
            .spawn_local(pick_rom(picked_roms.clone()));
    });

    button.add_event_listener_with_callback("click", listener.as_ref().unchecked_ref())?;
    listener.forget();

    Ok(())
}

async fn pick_rom(picked_roms: Sender<PathBuf>) {
    let Some(file) = AsyncFileDialog::new().pick_file().await else {
        return;
    };

    let rom = file.read().await;

    let (rom, rom_id) = match tokio::task::spawn_blocking(move || {
        let rom_id = RomId::new(rom.as_slice());
        (rom, rom_id)
    })
    .await
    {
        Ok((rom, Ok(rom_id))) => (rom, rom_id),
        Ok((_, Err(err))) => {
            tracing::error!("Could not hash {}: {}", file.file_name(), err);
            return;
        }
        Err(err) => {
            tracing::error!("Could not hash {}: {}", file.file_name(), err);
            return;
        }
    };

    let directory = STORAGE_DIRECTORY.join("roms");
    let path = directory.join(rom_id.to_string());

    if let Err(err) = tokio::fs::create_dir_all(&directory).await {
        tracing::error!("Could not create {}: {}", directory.display(), err);
        return;
    }

    if tokio::fs::try_exists(&path).await.unwrap_or(false) {
        let _ = picked_roms.send(path);
        return;
    }

    if let Err(err) = tokio::fs::write(&path, rom).await {
        tracing::error!("Could not store {}: {}", path.display(), err);
        return;
    }

    let _ = picked_roms.send(path);
}
