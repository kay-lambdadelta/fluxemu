# FluxEMU Nuttallite

This is the shell implementation for Apache NuttX supported boards

These dependencies are required.

| Distro | Development Package Name                                                       |
| ------ | ------------------------------------------------------------------------------ |
| Debian | `rustup python3-kconfiglib genromfs xxd libclang-dev build-essential rsync jq` |

You may need other dependencies depending on your board, and will almost certainly need a matching C toolchain.

## Building

To build, consult the options given by

```bash
cargo -p xtask build-nuttallite -- --help
```
