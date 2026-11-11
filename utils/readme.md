# FluxEMU Utils

This is the command line utility interface for FluxEMU, with tools that are also generically useful for ROM organization.

## Dependencies

See the [top level readme](../readme.md#toolchain) for acquiring the Rust toolchain.

### Linux

The instructions for listed distros should also work for their downstream distros (e.g. the Debian instructions work for Ubuntu).

| Distro   | Development Package Name |
| -------- | ------------------------ |
| Debian   | `libbz2-dev`             |
| Fedora   | `bzip2-devel`            |
| Arch     | `bzip2`                  |
| openSUSE | `libbz2-devel`           |

## MSRV (Minimum Supported Rust Version)

This follows the [`rust-toolchain.toml`](../shell/destinezite/rust-toolchain.toml) specified by [destinezite](../shell/destinezite). See the [top level readme](../readme.md#toolchain) for details.

## Building

```bash
cargo build -p fluxemu-utils --release
```

### ROM store backends

ROM stores are accessed through [OpenDAL](https://crates.io/crates/opendal).

The full list of backends is available in [OpenDAL's documentation](https://docs.rs/opendal/latest/opendal/services/index.html). See the [top level readme](../readme.md#rom-store-abstraction) for how to enable and configure them.

## Packaging

### Debian

With the [cargo-deb](https://crates.io/crates/cargo-deb) helper program, you can run this command to produce a Debian package:

```bash
cargo deb -p fluxemu-utils
```

It will detect link time dependencies automatically.

## Usage

The program is split into `database`, `rom`, `manifest` and `logiqx` subcommands.

```bash
fluxemu-utils --help
```

or (from the project directory)

```bash
cargo run -p fluxemu-utils --release -- --help
```
