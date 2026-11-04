# FluxEMU Nuttallite

This is the shell implementation for Apache NuttX supported boards

These dependencies are required.

| Distro | Development Package Name                                         |
| ------ | ---------------------------------------------------------------- |
| Debian | `rustup libclang-dev build-essential rsync jq kconfig-frontends` |

You may need other dependencies depending on your board, and will almost certainly need a matching C toolchain.

For example, to build for the Raspberry Pi Pico/Pico 2 you will need

| Distro | Development Package Name     |
| ------ | ---------------------------- |
| Debian | `gcc-arm-none-eabi picotool` |

## Building

Building uses the [cargo nuttx](https://crates.io/crates/cargo-nuttx) helper tool, install it with

```sh
cargo install cargo-nuttx --locked
```

To build, switch the workspace to the nuttallite toolchain and run cargo nuttx

```sh
cargo run -p xtask -- switch nuttallite
cargo nuttx build -b raspberrypi-pico-2 --release
```

The firmware ends up in `target/nuttx/<board>/<profile>/`, for example `target/nuttx/raspberrypi-pico-2/release/nuttx.uf2`.
