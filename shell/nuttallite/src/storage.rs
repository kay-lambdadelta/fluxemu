use std::{
    ffi::{CStr, CString, c_void},
    os::unix::ffi::OsStrExt,
};

use fluxemu_paths::STORAGE_DIRECTORY;

use crate::sys;

// These were all the mount points I could find in the NuttX tree
const BOARD_SD_CARD_MOUNT_POINTS: &[&CStr] =
    &[c"/mnt/sd", c"/mnt/sd0", c"/mnt/sdcard", c"/mnt/sdcard0"];
const SD_CARD_DEVICE: &CStr = c"/dev/mmcsd0";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StorageBacking {
    SdCard,
    Tmpfs,
}

#[derive(Debug, Clone, Copy)]
struct Partition {
    index: usize,
    first_block: usize,
    block_count: usize,
}

pub fn mount_storage_partition() -> Result<StorageBacking, std::io::Error> {
    let target = CString::new(STORAGE_DIRECTORY.as_os_str().as_bytes()).unwrap();

    for mount_point in BOARD_SD_CARD_MOUNT_POINTS {
        let _ = unsafe { sys::umount2(mount_point.as_ptr(), 0) };
    }

    if mount(Some(SD_CARD_DEVICE), &target, c"vfat").is_ok() {
        return Ok(StorageBacking::SdCard);
    }

    // The FAT driver doesn't really make any attempt to scan for GPT, so we do it for it
    for partition in gpt_partitions(SD_CARD_DEVICE) {
        let Some(partition_device) = register_partition(SD_CARD_DEVICE, partition) else {
            continue;
        };

        if mount(Some(&partition_device), &target, c"vfat").is_ok() {
            return Ok(StorageBacking::SdCard);
        }
    }

    mount(None, &target, c"tmpfs")?;

    Ok(StorageBacking::Tmpfs)
}

fn gpt_partitions(device: &CStr) -> Vec<Partition> {
    unsafe extern "C" fn collect_partition(
        partition: *mut sys::partition_s,
        argument: *mut c_void,
    ) {
        let partitions = unsafe { &mut *argument.cast::<Vec<_>>() };
        let partition = unsafe { &*partition };

        partitions.push(Partition {
            index: partition.index,
            first_block: partition.firstblock,
            block_count: partition.nblocks,
        });
    }

    let mut partitions = Vec::new();

    let result = unsafe {
        sys::parse_block_partition(
            device.as_ptr(),
            Some(collect_partition),
            (&raw mut partitions).cast(),
        )
    };

    if result < 0 {
        return Vec::new();
    }

    partitions
}

fn register_partition(parent: &CStr, partition: Partition) -> Option<CString> {
    let mut path = parent.to_bytes().to_vec();
    path.extend_from_slice(format!("p{}", partition.index).as_bytes());
    let path = CString::new(path).unwrap();

    let result = unsafe {
        sys::register_blockpartition(
            path.as_ptr(),
            0o660,
            parent.as_ptr(),
            partition.first_block.try_into().ok()?,
            partition.block_count.try_into().ok()?,
        )
    };

    if result < 0 && result != -(libc::EEXIST) {
        return None;
    }

    Some(path)
}

fn mount(source: Option<&CStr>, target: &CStr, filesystem_type: &CStr) -> std::io::Result<()> {
    let result = unsafe {
        sys::mount(
            source.map_or(std::ptr::null(), CStr::as_ptr),
            target.as_ptr(),
            filesystem_type.as_ptr(),
            0,
            std::ptr::null(),
        )
    };

    if result != 0 {
        return Err(std::io::Error::last_os_error());
    }

    Ok(())
}
