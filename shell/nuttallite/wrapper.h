#include <nuttx/fs/fs.h>
#include <nuttx/video/fb.h>
#include <nuttx/input/touchscreen.h>
// HACK: Rust libc pollfd is wrong for NuttX
#include <poll.h>
#include <sys/mman.h>
#include <sys/mount.h>

#ifndef FBIO_UPDATE
#define FBIO_UPDATE _FBIOC(0x0007)
#endif
