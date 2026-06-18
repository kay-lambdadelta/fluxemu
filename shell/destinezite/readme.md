# FluxEMU Destinezite

This is the shell implementation for desktop type platforms.

It supports the vast majority of popular environments, including Linux, Windows, MacOS, and the open source \*BSDs.

## Dependencies

The rust toolchain itself can be acquired via the instructions at the [official rust website](https://rust-lang.org/tools/install).

Alternatively, on Linux, you can use your distribution's package manager to install the rust toolchain, if your distro provides a new enough build.

### Windows and MacOS

This program should not have any further dependencies on these platforms, apart from the SDKs you would need to compile C/C++ programs, and the rust toolchain itself.

### Linux

The instructions for listed distros should work for downstream distros also (ie: the Debian instructions operate for Ubuntu).

| Distro   | Development Package Name                                             |
| -------- | -------------------------------------------------------------------- |
| Debian   | `libasound2-dev libudev-dev pkg-config build-essential`              |
| Fedora   | `alsa-lib-devel systemd-devel pkgconf-pkg-config @development-tools` |
| Arch     | `alsa-lib systemd base-devel`                                        |
| openSUSE | `alsa-lib-devel libudev-devel pkg-config`                            |

Feature specific dependencies (`webgpu` and `windowing` are on by default, `drm` is not):

#### `webgpu` — WebGPU backend for graphics rendering (via [wgpu](https://crates.io/crates/wgpu))

| Distro   | Development Package Name               |
| -------- | -------------------------------------- |
| Debian   | `libvulkan-dev libgl-dev`              |
| Fedora   | `vulkan-loader-devel mesa-libGL-devel` |
| Arch     | `vulkan-devel libglvnd`                |
| openSUSE | `vulkan-devel Mesa-libGL-devel`        |

#### `windowing` — windowing backend for interface display (via [winit](https://crates.io/crates/winit))

| Distro   | Development Package Name                        |
| -------- | ----------------------------------------------- |
| Debian   | `libx11-dev libxkbcommon-dev libwayland-dev`    |
| Fedora   | `libX11-devel libxkbcommon-devel wayland-devel` |
| Arch     | `libx11 libxkbcommon wayland`                   |
| openSUSE | `libX11-devel libxkbcommon-devel wayland-devel` |

#### `drm` — DRM backend for graphics rendering (linux only)

| Distro   | Development Package Name                          |
| -------- | ------------------------------------------------- |
| Debian   | `libinput-dev libxkbcommon-dev libseat-dev`       |
| Fedora   | `libinput-devel libxkbcommon-devel libseat-devel` |
| Arch     | `libinput libxkbcommon libseat`                   |
| openSUSE | `libinput-devel libxkbcommon-devel libseat-devel` |

## MSRV (Minimal Supported Rust Version)

Check the `rust-toolchain.toml` file at the project root for the minimium version of the rust compiler required for compilation. Older versions of the rust toolchain may compile and produce a operational program, but they are not supported or tested.

This compiler version will attempt to keep at or behind the version of [`rustc`](https://packages.debian.org/sid/rustc) packaged by Debian Sid.

## Building

To build, run one of the following commands:

### Linux

```bash
cargo build -p fluxemu-shell-destinezite --release --features drm
```

Note the `drm` feature is not strictly required for Linux, and not enabling it can drop some dependencies (listed above).

The `drm` feature also requires a working Vulkan implementation (for WebGPU support), or the fallback software renderer (always enabled).

### Others

```bash
cargo build -p fluxemu-shell-destinezite --release
```

## Packaging

### Debian

With the [cargo-deb](https://crates.io/crates/cargo-deb) helper program, you can run this command to produce a debian package.

```bash
cargo deb -p fluxemu-shell-destinezite --features drm
```

It will detect link time dependencies automatically.

## Usage

```bash
$EXECUTABLE_PATH --help
```

or (on the project directory)

```bash
cargo run -p fluxemu-shell-destinezite --release -- --help
```
