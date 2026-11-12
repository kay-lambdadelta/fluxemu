use std::{
    ffi::{CString, c_char, c_int, c_void},
    os::unix::ffi::OsStrExt,
    path::Path,
};

use fluxemu_paths::STORAGE_DIRECTORY;
use wasm_bindgen::JsValue;
use web_sys::js_sys::{Reflect, global};

unsafe extern "C" {
    fn wasmfs_create_opfs_backend() -> *mut c_void;
    fn wasmfs_create_directory(path: *const c_char, mode: u32, backend: *mut c_void) -> c_int;
}

#[derive(Debug, thiserror::Error)]
pub enum StorageError {
    #[error("This environment has no origin private file system")]
    Unavailable,
    #[error("Could not mount the origin private file system (error code {0})")]
    Mount(c_int),
}

pub fn mount_storage() -> Result<(), StorageError> {
    if !opfs_available() {
        return Err(StorageError::Unavailable);
    }

    mount_opfs(&STORAGE_DIRECTORY)
}

fn opfs_available() -> bool {
    let get_directory = ["navigator", "storage", "getDirectory"]
        .into_iter()
        .try_fold(JsValue::from(global()), |object, property| {
            Reflect::get(&object, &property.into())
                .ok()
                .filter(|value| !value.is_undefined() && !value.is_null())
        });

    get_directory.is_some_and(|get_directory| get_directory.is_function())
}

fn mount_opfs(target: &Path) -> Result<(), StorageError> {
    let target = CString::new(target.as_os_str().as_bytes()).unwrap();
    let result =
        unsafe { wasmfs_create_directory(target.as_ptr(), 0o777, wasmfs_create_opfs_backend()) };

    if result != 0 {
        return Err(StorageError::Mount(result));
    }

    Ok(())
}
