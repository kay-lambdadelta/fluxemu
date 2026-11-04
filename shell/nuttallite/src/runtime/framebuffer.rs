use std::{
    fs::{File, OpenOptions},
    io,
    mem::{self, MaybeUninit},
    os::fd::AsRawFd,
    ptr, slice,
};

use fluxemu_graphics::{rgb565::Rgb565Order, texture::RefMutTexture};
use nalgebra::Vector2;
use palette::cast::Packed;
use thiserror::Error;

use crate::sys;

// FIXME: Many of the stuff we are pulling from sys::* should probably be in libc::*

const DEVICE_PATH: &str = "/dev/fb0";
const MAP_FAILED: usize = usize::MAX;

pub type Pixel = Packed<Rgb565Order, u16>;

#[derive(Debug, Error)]
pub enum FramebufferError {
    #[error("Could not access framebuffer device: {0}")]
    Device(#[from] io::Error),
    #[error("Unsupported framebuffer pixel format {0}")]
    UnsupportedFormat(u8),
    #[error("Unsupported framebuffer stride of {0} bytes")]
    UnsupportedStride(u16),
}

pub struct Framebuffer {
    pages: Vec<RefMutTexture<'static, Pixel>>,
    back_page: usize,
    file: File,
    area: sys::fb_area_s,
    plane_info: sys::fb_planeinfo_s,
}

impl Framebuffer {
    pub fn open() -> Result<Self, FramebufferError> {
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .open(DEVICE_PATH)?;

        let video_info: sys::fb_videoinfo_s = query(&file, sys::FBIOGET_VIDEOINFO)?;
        let plane_info: sys::fb_planeinfo_s = query(&file, sys::FBIOGET_PLANEINFO)?;

        // We only support RGB565 for now
        if u32::from(video_info.fmt) != sys::FB_FMT_RGB16_565 {
            return Err(FramebufferError::UnsupportedFormat(video_info.fmt));
        }

        if !usize::from(plane_info.stride).is_multiple_of(size_of::<Pixel>()) {
            return Err(FramebufferError::UnsupportedStride(plane_info.stride));
        }

        let dimensions = Vector2::new(video_info.xres as usize, video_info.yres as usize);
        let stride = usize::from(plane_info.stride) / size_of::<Pixel>();

        let page_length = stride * dimensions.y;
        let page_count = (plane_info.yres_virtual as usize / dimensions.y)
            .min(plane_info.fblen / (page_length * size_of::<Pixel>()))
            .max(1);

        let mapping = unsafe {
            libc::mmap(
                ptr::null_mut(),
                plane_info.fblen,
                (sys::PROT_READ | sys::PROT_WRITE) as i32,
                sys::MAP_SHARED as i32,
                file.as_raw_fd(),
                0,
            )
        };

        if mapping.addr() == MAP_FAILED {
            return Err(io::Error::last_os_error().into());
        }

        let mapping = mapping.cast::<Pixel>();

        assert!(mapping.is_aligned());

        let pixels = unsafe { slice::from_raw_parts_mut(mapping, page_length * page_count) };

        let pages = pixels
            .chunks_exact_mut(page_length)
            .map(|page| {
                RefMutTexture::from_storage(stride, dimensions.y, page)
                    .view_owned(..dimensions.x, ..)
            })
            .collect();

        Ok(Self {
            pages,
            back_page: page_count - 1,
            file,
            area: sys::fb_area_s {
                x: 0,
                y: 0,
                w: video_info.xres,
                h: video_info.yres,
            },
            plane_info,
        })
    }

    pub fn back_page(&mut self) -> &mut RefMutTexture<'static, Pixel> {
        &mut self.pages[self.back_page]
    }

    pub fn front_page(&self) -> &RefMutTexture<'static, Pixel> {
        let page_count = self.pages.len();

        &self.pages[(self.back_page + page_count - 1) % page_count]
    }

    pub fn wait_for_back_page(&self) -> Result<(), io::Error> {
        if self.pages.len() == 1 {
            return Ok(());
        }

        let mut poll_descriptor: sys::pollfd = unsafe { mem::zeroed() };
        poll_descriptor.fd = self.file.as_raw_fd();
        poll_descriptor.events = libc::POLLOUT as _;

        loop {
            if unsafe { sys::poll(&raw mut poll_descriptor, 1, -1) } >= 0 {
                return Ok(());
            }

            let error = io::Error::last_os_error();

            if error.raw_os_error() != Some(libc::EINTR) {
                return Err(error);
            }
        }
    }

    pub fn present(&mut self) -> Result<(), io::Error> {
        if self.pages.len() == 1 {
            return self.update();
        }

        self.plane_info.yoffset = self.back_page as u32 * u32::from(self.area.h);

        let result = unsafe {
            libc::ioctl(
                self.file.as_raw_fd(),
                sys::FBIOPAN_DISPLAY as i32,
                &raw const self.plane_info,
            )
        };

        if result < 0 {
            return Err(io::Error::last_os_error());
        }

        self.back_page = (self.back_page + 1) % self.pages.len();

        Ok(())
    }

    fn update(&self) -> Result<(), io::Error> {
        let result = unsafe {
            libc::ioctl(
                self.file.as_raw_fd(),
                sys::FBIO_UPDATE as i32,
                &raw const self.area,
            )
        };

        if result < 0 {
            let error = io::Error::last_os_error();

            if error.raw_os_error() != Some(libc::ENOTTY) {
                return Err(error);
            }
        }

        Ok(())
    }
}

impl Drop for Framebuffer {
    fn drop(&mut self) {
        let mapping = self.pages[0].storage_mut().as_mut_ptr();

        unsafe { libc::munmap(mapping.cast(), self.plane_info.fblen) };
    }
}

fn query<T>(file: &File, request: u32) -> Result<T, io::Error> {
    let mut value = MaybeUninit::<T>::zeroed();

    let result = unsafe { libc::ioctl(file.as_raw_fd(), request as i32, value.as_mut_ptr()) };

    if result < 0 {
        return Err(io::Error::last_os_error());
    }

    Ok(unsafe { value.assume_init() })
}
