# FluxEMU Runtime

FluxEMU Runtime defines the core glue between guest system code and host system code.

It implements numerous utilities to construct well defined guest machines and execute them in a way that is suitable for reasonable frontend/shell implementation, regardless of threading or environment implementation.

## Core design

The runtime centers around a `Machine` struct, which encapsulates the guest machine's hardware description/state. The machine is centered around a collection of the runtime's most fundamental unit, a `Component`, which is a trait containing some basic runtime required methods, and represents individual or logically grouped hardware devices.

Components can only be accessed via methods on a lock free registry, which ensures they are caught up to the given timestamp before allowing access, enforcing that all code obeys the scheduler model of the emulator. The runtime has no concept of frequency to run by, instead asking components to strategize updating their state to the current time in a hardware accurate way via explicit time allocation.

With this, the emulator enforces a "lazy execution" model, where the vast majority of components don't run code in lockstep with each other, but only when they are actually accessed, allowing for more efficient batching of emulator operations around actual guest machine/program access patterns. Additionally, component synchronization can be cooperatively preempted by events, which helps implement periodic state changes not based upon a direct flow of execution from component access to component access, and cuts down on cycle by cycle condition checking.

The runtime fully anticipates multithreaded shells/frontends, so with a machine composed of well behaved components, all emulator functions can be accessed from any thread at any time simultaneously or not, and maintain accuracy and emulation determinism.
It tries to remain as lock free and as fast as possible, to maximize multithread capabilities without sacrificing single thread throughput.

Memory is managed via a page table model, where each individual address space (which there can be as many as required) is divided into fixed size segments, splitting memory map entries that exist in those segments into buckets that are scanned linearly. Mutable, immutable, and complex component backed memory are represented in this table format.
Modifying memory mappings does not modify this page table however, but modify a efficient range indexed interval tree like structure, and batched modification operations are compiled into the page tables used for lookup.
