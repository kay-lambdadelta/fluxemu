use core::ffi::{CStr, c_int};

use crate::sys::{LOG_ERR, syslog};

pub fn error(message: &CStr, code: c_int) {
    unsafe {
        syslog(
            LOG_ERR as c_int,
            c"ERROR: %s: %d\n".as_ptr(),
            message.as_ptr(),
            code,
        )
    };
}
