# FluxEMU Runtime

FluxEMU Runtime defines the core glue between guest system code and host system code.

It implements numerous utilities to construct well defined guest machines and execute them in a way that is suitable for reasonable frontend/shell implementation, regardless of threading or environment implementation.

## Summary of operation and concepts

The runtime centers around a `Machine` struct, which encapsulates the guest machine's hardware description/state. The machine is centered around a collection of the runtime's most fundamental unit, a `Component`, which is a trait containing some basic runtime required methods, and represents individual or logically grouped hardware devices.

Components can only be accessed via methods on a lock free registry, which makes sure they are caught up to a passed in timestamp before allowing access, enforcing that all code obeys the scheduler model of the emulator. The runtime has no concept of "frequency" to run by, instead asking components to strategize "catching up" their state to the current time in a hardware accurate way via time allocation and other synchronization related functions.

With this, the emulator enforces a "lazy execution" model, where the vast majority of components don't run code in lockstep with each other, but only when they are actually accessed, allowing for more efficient batching of emulator operations around actual guest machine/program access patterns. Additionally, component synchronization can be preempted by events, which helps implement periodic state changes not based upon a direct flow of execution from component access to component access, and cuts down on cycle by cycle condition checking.

The runtime fully anticipates multithreaded shells/frontends, so with a machine composed of components that well describe their model within the scheduler model of the runtime, components and runtime utilities can be accessed from any thread at any time simultaneously or not, and maintain accuracy and emulation determinism. The runtime tries to remain as lock free and as fast as possible, to maximize multi thread capabilities without sacrificing single thread throughput.
