# FluxEMU [Weberite](https://www.mindat.org/min-4253.html)

This is the shell implementation for web browsers, built around wasm and Emscripten.

> [!WARNING]
> This shell is an early prototype. It can load a ROM through a file picker, display the machine on a canvas, and take keyboard input, anything else is unimplemented.
>
> Additionally, Emscripten has only recently started to pick up in the Rust ecosystem, and the shell might hit rough edges among browsers and ecosystem crates.

## Dependencies

The Rust toolchain must be installed via rustup (see the [top level readme](../../readme.md#toolchain)), since this shell builds against a pinned nightly toolchain that distribution packaged `rustc` builds will not provide.

### Linux

The instructions for listed distros should also work for their downstream distros (e.g. the Debian instructions work for Ubuntu).

| Distro | Development Package Name                       |
| ------ | ---------------------------------------------- |
| Debian | `rustup curl build-essential python3 xz-utils` |

### Emscripten

Building needs the [Emscripten SDK](https://emscripten.org/docs/getting_started/downloads.html), at the version listed in [`emsdk-version`](emsdk-version). Distribution packaged builds of Emscripten are generally too old, and will probably have mysterious interactions with Rust wasm ecosystem crates.

```sh
git clone https://github.com/emscripten-core/emsdk.git
cd emsdk
./emsdk install "$(cat <fluxemu>/shell/weberite/emsdk-version)"
./emsdk activate "$(cat <fluxemu>/shell/weberite/emsdk-version)"
source ./emsdk_env.sh
```

### wasm-bindgen

Emscripten runs the [wasm-bindgen](https://crates.io/crates/wasm-bindgen-cli) command line tool from `PATH` while linking, and its version must match the `wasm-bindgen` crate version in the workspace's `Cargo.lock`:

```sh
cargo install wasm-bindgen-cli --locked --version "$(cargo pkgid wasm-bindgen | cut -d@ -f2)"
```

### Feature specific dependencies

> [!NOTE]
> No features are on by default.

#### `webgpu` — WebGPU backend for graphics rendering (via [wgpu](https://crates.io/crates/wgpu))

> [!CAUTION]
> This feature does not currently build.

## Toolchain

This shell uses its own [`rust-toolchain.toml`](rust-toolchain.toml), pinned to a specific nightly, and its own [Cargo configuration](.cargo/config.toml), rather than the ones the rest of the workspace uses.

The workspace has to be switched over to them before building:

```sh
cargo run -p xtask -- switch weberite
```

> [!IMPORTANT]
> This affects every other crate built from the workspace until you switch back. See the [top level readme](../../readme.md#toolchain) for details.

## Building

With the workspace switched to this shell, run:

```sh
cargo build -p fluxemu-shell-weberite --target wasm32-unknown-emscripten --release
```

This produces `fluxemu-shell-weberite.js` and `fluxemu_shell_weberite.wasm` in `target/wasm32-unknown-emscripten/release/`.

Copy both next to [`web/index.html`](web/index.html) to get a servable site:

```sh
mkdir -p target/weberite-site
cp shell/weberite/web/index.html \
  external/coi-serviceworker/coi-serviceworker.js \
  target/wasm32-unknown-emscripten/release/fluxemu-shell-weberite.js \
  target/wasm32-unknown-emscripten/release/fluxemu_shell_weberite.wasm \
  target/weberite-site/
```

## Usage

The shell relies on wasm threads, which browsers only allow on [cross-origin isolated](https://developer.mozilla.org/en-US/docs/Web/API/Window/crossOriginIsolated) pages. Whatever serves the site must send these headers:

```
Cross-Origin-Opener-Policy: same-origin
Cross-Origin-Embedder-Policy: require-corp
```

If the host can't send them (e.g. GitHub Pages), the page falls back to [coi-serviceworker](https://github.com/gzuidhof/coi-serviceworker), which adds them from a service worker. This costs one extra reload on the first visit, and may not work correctly in some browsers.

Emscripten's `emrun` sends them correctly, so for local testing you can run:

```sh
emrun --no-browser --hostname localhost --port 3434 target/weberite-site/index.html
```

and open <http://localhost:3434/index.html>.

### Storage

Persistent data (configuration, the program database, and imported ROMs) is stored in the browser's [Origin Private File System](https://developer.mozilla.org/en-US/docs/Web/API/File_System_API/Origin_private_file_system).

> [!NOTE]
> Firefox does not yet support shared OPFS access handles, so the site can only be open in one Firefox tab at a time.
>
> It will most likely freeze if you try.
