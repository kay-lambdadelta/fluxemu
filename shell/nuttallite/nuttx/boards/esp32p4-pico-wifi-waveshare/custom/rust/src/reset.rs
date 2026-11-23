use core::ffi::c_int;

use crate::sys::{up_shutdown_handler, up_systemreset};

#[unsafe(no_mangle)]
pub extern "C" fn board_reset(status: c_int) -> c_int {
    if status == 0 {
        unsafe { up_shutdown_handler() };
    }

    unsafe { up_systemreset() }
}
