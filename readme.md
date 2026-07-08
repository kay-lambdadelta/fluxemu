# FluxEMU

FluxEMU is a prototype multisystem hardware emulation framework written in Rust.

The goal is to create a framework that can represent _any_ hardware system with as little hacks or workarounds as possible,
with full thread safety, without sacrificing performance, host system agnosticity, or user convenience.

Any hardware component or system should be implementable and share the same substrate of flexible and fast [runtime](lib/runtime) code.

For further details check the `readme.md` files scattered among the crates in the repository.
