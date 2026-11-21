use std::{
    fs::{File, OpenOptions},
    io,
    os::{fd::AsRawFd, unix::fs::OpenOptionsExt},
    ptr, thread,
    time::{Duration, Instant},
};

use crate::sys;

// NOTE: NuttX seems to disable interrupts upon crashes
//
// This means logs will probably be cut off upon crashes

const DEVICE_PATH: &str = "/dev/console";
const MINOR: i32 = 0;
const ENUMERATION_TIMEOUT: Duration = Duration::from_secs(3);
const ENUMERATION_POLL_INTERVAL: Duration = Duration::from_millis(50);

pub fn attach_usb_console() -> Result<(), io::Error> {
    let result = unsafe { sys::cdcacm_initialize(MINOR, ptr::null_mut()) };

    if result < 0 {
        return Err(io::Error::from_raw_os_error(-result));
    }

    let input = wait_for_host()?;
    let output = OpenOptions::new()
        .write(true)
        .custom_flags(libc::O_NONBLOCK)
        .open(DEVICE_PATH)?;

    for (file, descriptor) in [
        (&input, libc::STDIN_FILENO),
        (&output, libc::STDOUT_FILENO),
        (&output, libc::STDERR_FILENO),
    ] {
        if unsafe { libc::dup2(file.as_raw_fd(), descriptor) } < 0 {
            return Err(io::Error::last_os_error());
        }
    }

    Ok(())
}

fn wait_for_host() -> Result<File, io::Error> {
    let start = Instant::now();

    loop {
        match OpenOptions::new().read(true).open(DEVICE_PATH) {
            Err(error)
                if error.raw_os_error() == Some(libc::ENOTCONN)
                    && start.elapsed() < ENUMERATION_TIMEOUT =>
            {
                thread::sleep(ENUMERATION_POLL_INTERVAL);
            }
            result => return result,
        }
    }
}
