use core::{ffi::c_int, panic::PanicInfo};

use crate::sys::__assert;

#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    let mut filename = [0; 64];
    let (path, line) = info.location().map_or(("unknown", 0), |location| {
        (location.file(), location.line())
    });
    let tail = &path.as_bytes()[path.len().saturating_sub(filename.len() - 1)..];
    filename[..tail.len()].copy_from_slice(tail);

    unsafe {
        __assert(
            filename.as_ptr().cast(),
            c_int::try_from(line).unwrap_or(c_int::MAX),
            c"rust panic".as_ptr(),
        )
    }
}
