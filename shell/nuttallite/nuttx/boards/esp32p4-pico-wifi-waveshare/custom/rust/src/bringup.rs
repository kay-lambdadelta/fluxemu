use core::{ffi::CStr, ptr};

use crate::{
    sd_card,
    sys::{CONFIG_LIBC_TMPDIR, nx_mount},
    syslog,
};

fn mount(target: &CStr, filesystem_type: &CStr, failure_message: &CStr) {
    let result = unsafe {
        nx_mount(
            ptr::null(),
            target.as_ptr(),
            filesystem_type.as_ptr(),
            0,
            ptr::null(),
        )
    };

    if result < 0 {
        syslog::error(failure_message, result);
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn esp_board_initialize() {}

#[unsafe(no_mangle)]
pub extern "C" fn board_late_initialize() {
    mount(c"/proc", c"procfs", c"Failed to mount procfs");
    mount(
        CStr::from_bytes_with_nul(CONFIG_LIBC_TMPDIR).unwrap(),
        c"tmpfs",
        c"Failed to mount tmpfs",
    );

    sd_card::initialize();
}
