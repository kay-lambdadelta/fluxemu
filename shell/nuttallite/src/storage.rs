use std::{ffi::CString, os::unix::ffi::OsStrExt};

use fluxemu_environment::STORAGE_DIRECTORY;

use crate::sys;

pub fn mount_storage_partition() -> Result<(), std::io::Error> {
    let target_bytes = STORAGE_DIRECTORY.as_os_str().as_bytes();

    let target = CString::new(target_bytes).unwrap();
    let fstype = CString::new("tmpfs").unwrap();

    let result = unsafe {
        sys::mount(
            std::ptr::null(),
            target.as_ptr(),
            fstype.as_ptr(),
            0,
            std::ptr::null(),
        )
    };

    if result != 0 {
        return Err(std::io::Error::last_os_error());
    }

    Ok(())
}
