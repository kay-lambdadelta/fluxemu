# FluxEMU

FluxEMU is a prototype multisystem hardware emulation framework written in Rust.

The goal is to create a framework that can represent _any_ hardware system with as little hacks or workarounds as possible,
with full thread safety, without sacrificing performance, host system independence, or user convenience.

Any hardware component or system should be implementable and share the same substrate of flexible and fast [runtime](lib/runtime) code.

For further details check the `readme.md` files scattered among the crates in the repository and/or continue reading.

## Global Project Information

### Test and Benchmark Builds

Test and benchmark builds embed third party test ROMs, many of which have unclear or restrictive licenses.

> [!CAUTION]
> Do not redistribute these builds. Please.

### Toolchain

The Rust toolchain itself can be acquired via the instructions at the [official Rust website](https://rust-lang.org/tools/install).

Alternatively, on Linux, you can use your distribution's package manager to install the Rust toolchain, if your distro provides a new enough build.

> [!TIP]
> On Debian you can achieve this via:
>
> ```sh
> sudo apt install rustup
> ```

The workspace follows the [`rust-toolchain.toml`](shell/destinezite/rust-toolchain.toml) specified by [destinezite](shell/destinezite), which is the minimum version of the Rust compiler required for compilation.

> [!WARNING]
> Older versions of the Rust toolchain may compile and produce an operational program, but they are not supported or tested.

The required version aims to stay at or behind the version of [`rustc`](https://packages.debian.org/sid/rustc) packaged by Debian Sid.

Some shells have stranger requirements and specify their own toolchain. The workspace can be switched over to one of these like so:

```sh
cargo run -p xtask -- switch nuttallite
```

> [!IMPORTANT]
> This relinks the workspace root's `rust-toolchain.toml` (and Cargo and editor configuration, if the shell has them), so it affects every other crate built from the workspace until you switch back with:
>
> ```sh
> cargo run -p xtask -- switch destinezite
> ```

### ROM Store Abstraction

This emulator uses [Apache OpenDAL](https://opendal.apache.org/) to support loading ROMs from a multitude of backends.

All leaf crates (the shell crates, and the util crates) support turning on individual OpenDAL backends, and individual backends can be [configured](https://opendal.apache.org/services/) in the GUIs or via the `environment.ron` file.

For example, this enables [HTTP](https://opendal.apache.org/services/http/) ROM stores for a build of [destinezite](shell/destinezite):

> [!IMPORTANT]
> Many HTTP based backends will implicitly require you to turn on `opendal/http-transport-reqwest`.
>
> It does not hurt anything except causing mild bloat to turn it on just in case with non default backend selections.

```sh
cargo build -p fluxemu-shell-destinezite --features opendal/http-transport-reqwest,opendal/services-http --release
```

Which then can be setup in your `environment.ron` like so:

```ron
(
    // Other parts of environment.ron removed for brevity
    rom_stores: {
        "test http store": (
            scheme: "http",
            options: {
                "endpoint": "http://127.0.0.1:8080",
            },
        ),
    },
)
```
