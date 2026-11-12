# FluxEMU [Destinezite](https://www.mindat.org/min-5222.html)

This is the shell implementation for desktop type platforms.

It supports the vast majority of popular environments, including Linux, Windows, macOS, and the open source \*BSDs, relying on common Rust ecosystem crates to achieve this.

## Dependencies

See the [top level readme](../../readme.md#toolchain) for acquiring the Rust toolchain.

### Windows and macOS

This program should not have any further dependencies on these platforms, apart from the SDKs you would need to compile C/C++ programs, and the Rust toolchain itself.

### Linux

The instructions for listed distros should also work for their downstream distros (e.g. the Debian instructions work for Ubuntu).

| Distro   | Development Package Name                                             |
| -------- | -------------------------------------------------------------------- |
| Debian   | `libasound2-dev libudev-dev pkg-config build-essential`              |
| Fedora   | `alsa-lib-devel systemd-devel pkgconf-pkg-config @development-tools` |
| Arch     | `alsa-lib systemd base-devel`                                        |
| openSUSE | `alsa-lib-devel libudev-devel pkg-config`                            |

Feature specific dependencies:

> [!NOTE]
> `webgpu`, `windowing` and `ratatui` are on by default, and `egui` comes with `windowing`. The rest are not.

#### `webgpu` — WebGPU backend for graphics rendering (via [wgpu](https://crates.io/crates/wgpu))

| Distro   | Development Package Name               |
| -------- | -------------------------------------- |
| Debian   | `libvulkan-dev libgl-dev`              |
| Fedora   | `vulkan-loader-devel mesa-libGL-devel` |
| Arch     | `vulkan-devel libglvnd`                |
| openSUSE | `vulkan-devel Mesa-libGL-devel`        |

#### `windowing` — Windowing backend for interface display (via [winit](https://crates.io/crates/winit))

| Distro   | Development Package Name                        |
| -------- | ----------------------------------------------- |
| Debian   | `libx11-dev libxkbcommon-dev libwayland-dev`    |
| Fedora   | `libX11-devel libxkbcommon-devel wayland-devel` |
| Arch     | `libx11 libxkbcommon wayland`                   |
| openSUSE | `libX11-devel libxkbcommon-devel wayland-devel` |

#### `drm` — DRM backend for interface display (Linux only)

| Distro   | Development Package Name                          |
| -------- | ------------------------------------------------- |
| Debian   | `libinput-dev libxkbcommon-dev libseat-dev`       |
| Fedora   | `libinput-devel libxkbcommon-devel libseat-devel` |
| Arch     | `libinput libxkbcommon libseat`                   |
| openSUSE | `libinput-devel libxkbcommon-devel libseat-devel` |

#### [`egui`](https://crates.io/crates/egui) — GUI frontend (enabled by `windowing` or `drm`)

| Distro   | Development Package Name                  |
| -------- | ----------------------------------------- |
| Debian   | `libfontconfig-dev fonts-noto-core`       |
| Fedora   | `fontconfig-devel google-noto-sans-fonts` |
| Arch     | `fontconfig noto-fonts`                   |
| openSUSE | `noto-sans-fonts`                         |

#### [`ratatui`](https://crates.io/crates/ratatui) — TUI frontend

##### Subfeatures

- `ratatui-image/chafa-dyn` — Enables chafa for potentially better rendering capabilities

| Distro   | Development Package Name |
| -------- | ------------------------ |
| Debian   | `libchafa-dev`           |
| Fedora   | `chafa-devel`            |
| Arch     | `chafa`                  |
| openSUSE | `chafa-devel`            |

## MSRV (Minimum Supported Rust Version)

Check the [`rust-toolchain.toml`](rust-toolchain.toml) file for the minimum version of the Rust compiler required for compilation. See the [top level readme](../../readme.md#toolchain) for the policy behind it.

## Building

To build, run one of the following commands:

### Linux

```bash
cargo build -p fluxemu-shell-destinezite --release --features drm
```

> [!TIP]
> The `drm` feature is not strictly required for Linux, and leaving it off drops some dependencies (listed above).

> [!IMPORTANT]
> The `drm` feature needs a working Vulkan implementation for WebGPU rendering. Without one, it falls back to the software renderer (always enabled, possibly slow).

### Others

```bash
cargo build -p fluxemu-shell-destinezite --release
```

### ROM store backends

ROM stores are accessed through [OpenDAL](https://crates.io/crates/opendal), and only the local filesystem backend (`services-fs`) is built in by default.

The full list of backends is available in [OpenDAL's documentation](https://docs.rs/opendal/latest/opendal/services/index.html). See the [top level readme](../../readme.md#rom-store-abstraction) for how to configure them.

## Packaging

### Debian

With the [cargo-deb](https://crates.io/crates/cargo-deb) helper program, you can run this command to produce a Debian package:

```bash
cargo deb -p fluxemu-shell-destinezite --features drm
```

It will detect link time dependencies automatically, and other dependencies are manually configured in [Cargo.toml](Cargo.toml)

## Usage

```bash
fluxemu-shell-destinezite --help
```

or (from the project directory)

```bash
cargo run -p fluxemu-shell-destinezite --release -- --help
```
