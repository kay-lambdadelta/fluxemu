# FluxEMU [Nuttallite](https://www.mindat.org/min-11140.html)

This is the shell implementation for [Apache NuttX](https://nuttx.apache.org/) supported boards.

> [!WARNING]
> This shell remains extremely experimental, and lives on the bleeding edge of not only this emulator, but Rust itself.
>
> It should work in some capacities, but for the time being, may break for specific boards, or entirely, and is deeply incomplete in its integration with NuttX.

## Dependencies

The Rust toolchain must be installed via rustup (see the [top level readme](../../readme.md#toolchain)), since this shell builds against a pinned nightly toolchain that distribution packaged `rustc` builds will not provide.

### Linux

The instructions for listed distros should also work for their downstream distros (e.g. the Debian instructions work for Ubuntu).

| Distro | Development Package Name                                         |
| ------ | ---------------------------------------------------------------- |
| Debian | `rustup libclang-dev build-essential rsync jq kconfig-frontends` |

### Board specific dependencies

You may need other dependencies depending on your board, and will almost certainly need a matching C toolchain.

#### `raspberrypi-pico-2` — Raspberry Pi Pico 2

| Distro | Development Package Name     |
| ------ | ---------------------------- |
| Debian | `gcc-arm-none-eabi picotool` |

### Feature specific dependencies

> [!NOTE]
> `egui` is on by default. The rest are not.

#### [`egui`](https://crates.io/crates/egui) — GUI frontend

#### [`ratatui`](https://crates.io/crates/ratatui) — TUI frontend

> [!CAUTION]
> This feature does not currently build for NuttX, as the [`libc`](https://crates.io/crates/libc) crate lacks some signal handling definitions for it.

## Toolchain

This shell uses its own [`rust-toolchain.toml`](rust-toolchain.toml), pinned to a specific nightly, rather than the one the rest of the workspace uses.

The workspace has to be switched over to it before building:

```sh
cargo run -p xtask -- switch nuttallite
```

> [!IMPORTANT]
> This affects every other crate built from the workspace until you switch back. See the [top level readme](../../readme.md#toolchain) for details.

## Building

Building uses the [cargo-nuttx](https://crates.io/crates/cargo-nuttx) helper program, which you can install with:

```sh
cargo install cargo-nuttx --locked
```

Then, with the workspace switched to this shell, run:

```sh
cargo nuttx build --board raspberrypi-pico-2 --release
```

The firmware ends up in `target/nuttx/<board>/<profile>/`, for example `target/nuttx/raspberrypi-pico-2/release/nuttx.uf2`.

### ROM store backends

ROM stores are accessed through [OpenDAL](https://crates.io/crates/opendal).

The full list of backends is available in [OpenDAL's documentation](https://docs.rs/opendal/latest/opendal/services/index.html). See the [top level readme](../../readme.md#rom-store-abstraction) for how to enable and configure them.

> [!WARNING]
> NuttX is an embedded OS, so many backends for OpenDAL may not fit into flash or will break or have no way to work at runtime.
>
> The story for how ROM loading or other aspects of persistent storage will be done on it is still being written. Submission of ideas is welcome.
