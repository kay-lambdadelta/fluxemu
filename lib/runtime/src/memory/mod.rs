use std::{
    fmt::Debug,
    hash::Hash,
    ops::RangeInclusive,
    sync::{Arc, Mutex, atomic::Ordering},
};

use bytes::Bytes;
use fluxemu_range::ContiguousRange;
use rangemap::RangeInclusiveMap;
pub(crate) use registry::{
    LocalMemoryRegistryData, MemoryId, MemoryRegistryData, RegionInitializationData,
};
use sdd::{AtomicOwned, Guard};
use thin_vec::ThinVec;
use thiserror::Error;

use crate::{
    ResourcePath, RuntimeHandle, component::ComponentId, memory::registry::MemoryRegistry,
    path::ComponentPath, scheduler::Period,
};

mod ops;
mod registry;
mod remap;
#[cfg(test)]
mod tests;

pub type Address = usize;
const CHUNK_SIZE: Address = 0x1000;
const MAX_MIRROR_DEPTH: usize = 4;

/// Handle to a address space specificed at machine registration time
///
/// This is the primary interface for accessing memory in the runtime
#[derive(Debug)]
pub struct AddressSpace<'a> {
    runtime: &'a RuntimeHandle,
    data: &'a AddressSpaceData,
    guard: Guard,
}

impl<'a> AddressSpace<'a> {
    #[inline]
    pub(crate) fn new(runtime: &'a RuntimeHandle, data: &'a AddressSpaceData) -> Self {
        Self {
            runtime,
            data,
            guard: Guard::new(),
        }
    }

    /// Modify the memory mapping based upon the command list given
    ///
    /// Note that:
    ///
    /// - Mapping changes are ADDITIVE, they apply on top of existing mappings
    /// - Within a given command set this is an atomic operation, however it does not block accesses to address space methods while it
    ///   is completing
    /// - If two remappings from different threads are done at the same time, its unspecified which one "wins"
    /// - As of the current implementation, remapping is somewhat expensive. Much of the overhead is from the overhead of the remap setup itself.
    ///   Group together commands into as large of lists as you can.
    #[inline]
    pub fn remap(
        &mut self,
        timestamp: &Period,
        commands: impl IntoIterator<Item = MemoryMapCommand>,
    ) {
        // Some programs who do extremely frequent remappings will inflate ram with old copies of the mapping table
        //
        // This informs `sdd` that cleanup should happen sooner rather than later
        self.guard.accelerate();

        self.data
            .remap(timestamp, &self.guard, self.runtime, commands);
    }
}

impl<'a> AddressSpace<'a> {
    pub fn id(&self) -> AddressSpaceId {
        self.data.id
    }
}

#[derive(Clone, Debug)]
enum PageTableTarget {
    ImmutableMemory(Bytes),
    Memory { offset: Address, id: MemoryId },
    Component { offset: Address, id: ComponentId },
}

#[derive(Debug, Clone)]
struct PageTableEntry {
    /// Full, uncropped relevant range
    pub range: std::range::RangeInclusive<Address>,
    pub target: PageTableTarget,
}

#[derive(Debug, Clone)]
struct PageTable(Box<[Arc<[PageTableEntry]>]>);

impl PageTable {
    pub fn new(address_space_width: u8) -> Self {
        let addr_space_size = 2usize.pow(u32::from(address_space_width));
        let total_pages = addr_space_size.div_ceil(CHUNK_SIZE);

        Self(vec![Default::default(); total_pages].into_boxed_slice())
    }
}

/// Identifier for a address space
#[derive(Debug, Copy, Clone, Eq, PartialEq, PartialOrd, Ord, Hash)]
pub struct AddressSpaceId(pub(crate) u16);

/// Why a memory operation failed
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum MemoryErrorType {
    /// Access was denied
    Denied,
    /// Nothing is mapped there
    OutOfBus,
    /// It would be impossible to view this memory without a state change
    ///
    /// Only applicable for read operations
    Impossible,
}

/// Wrapper around the error type in order to specify ranges
#[derive(Error)]
#[error("Memory operation failed: {0:#x?}")]
pub struct MemoryError(pub ThinVec<(RangeInclusive<Address>, MemoryErrorType)>);

impl Debug for MemoryError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("MemoryError")
            .field(&format_args!(
                "{:x?}",
                RangeInclusiveMap::from_iter(self.0.clone())
            ))
            .finish()
    }
}

#[derive(Debug, Default)]
struct MasterTables {
    read: RangeInclusiveMap<Address, MasterTableEntry>,
    write: RangeInclusiveMap<Address, MasterTableEntry>,
}

#[derive(Debug, Clone)]
enum MasterTableEntry {
    Memory {
        path: ResourcePath,
        source_base: Address,
        region_base: Address,
        length: usize,
    },
    ImmutableMemory(Bytes),
    Component(ComponentPath),
    Mirror {
        source_base: Address,
        destination_base: Address,
    },
}

impl PartialEq for MasterTableEntry {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Component(a), Self::Component(b)) => a == b,
            (
                Self::Mirror {
                    source_base: source_base_a,
                    destination_base: destination_base_a,
                },
                Self::Mirror {
                    source_base: source_base_b,
                    destination_base: destination_base_b,
                },
            ) => source_base_a == source_base_b && destination_base_a == destination_base_b,
            (
                Self::Memory {
                    path: path_a,
                    source_base: source_base_a,
                    region_base: region_base_a,
                    length: length_a,
                },
                Self::Memory {
                    path: path_b,
                    source_base: source_base_b,
                    region_base: region_base_b,
                    length: length_b,
                },
            ) => {
                path_a == path_b
                    && source_base_a == source_base_b
                    && region_base_a == region_base_b
                    && length_a == length_b
            }
            _ => false,
        }
    }
}

impl Eq for MasterTableEntry {}

#[derive(Debug)]
pub(crate) struct AddressSpaceData {
    width_mask: Address,
    address_space_width: u8,
    id: AddressSpaceId,
    read_table: AtomicOwned<PageTable>,
    write_table: AtomicOwned<PageTable>,
    master: Mutex<MasterTables>,
}

impl AddressSpaceData {
    pub(crate) fn new(id: AddressSpaceId, width: u8) -> Self {
        assert!(
            width as usize <= usize::BITS as usize,
            "width exceeds usize::BITS"
        );

        let width_mask = (1usize << width).wrapping_sub(1);

        Self {
            id,
            width_mask,
            address_space_width: width,
            read_table: AtomicOwned::new(PageTable::new(width)),
            write_table: AtomicOwned::new(PageTable::new(width)),
            master: Mutex::default(),
        }
    }

    #[inline]
    fn get_read_table<'a>(&'a self, guard: &'a Guard) -> &'a PageTable {
        // SAFETY: We never set an null members mapping, and we don't set any tag bits
        unsafe {
            self.read_table
                .load(Ordering::Acquire, guard)
                .as_ref_unchecked()
                .unwrap_unchecked()
        }
    }

    #[inline]
    fn get_write<'a>(&'a self, guard: &'a Guard) -> &'a PageTable {
        // SAFETY: We never set an null members mapping, and we don't set any tag bits
        unsafe {
            self.write_table
                .load(Ordering::Acquire, guard)
                .as_ref_unchecked()
                .unwrap_unchecked()
        }
    }
}

#[derive(Debug, Clone)]
pub enum MapTarget {
    Component(ComponentPath),
    Memory {
        path: ResourcePath,
        subrange: Option<RangeInclusive<Address>>,
    },
    ImmutableMemory(Bytes),
    Mirror {
        destination: RangeInclusive<Address>,
    },
}

/// Command for how the memory access table should modify the memory map
#[allow(missing_docs)]
#[derive(Debug, Clone)]
pub enum MemoryMapCommand {
    /// Add a target to the memory map, or add a map to an existing one
    Map {
        range: RangeInclusive<Address>,
        permissions: Permissions,
        target: MapTarget,
    },
    /// Clear a memory range
    Unmap {
        range: RangeInclusive<Address>,
        permissions: Permissions,
    },
}

impl MemoryMapCommand {
    pub fn with_component(
        path: ComponentPath,
        input: impl IntoIterator<Item = (RangeInclusive<Address>, Permissions)>,
    ) -> impl Iterator<Item = Self> {
        input
            .into_iter()
            .map(move |(range, permissions)| Self::Map {
                permissions,
                range,
                target: MapTarget::Component(path.clone()),
            })
    }

    pub fn mirror(
        permissions: Permissions,
        source: RangeInclusive<Address>,
        destination: Address,
    ) -> Self {
        Self::Map {
            permissions,
            target: MapTarget::Mirror {
                destination: RangeInclusive::from_start_and_length(destination, source.len()),
            },
            range: source,
        }
    }

    pub fn immutable_memory(base: Address, buffer: impl Into<Bytes>) -> Self {
        let buffer = buffer.into();

        Self::Map {
            permissions: Permissions::READ,
            range: RangeInclusive::from_start_and_length(base, buffer.len()),
            target: MapTarget::ImmutableMemory(buffer),
        }
    }

    pub fn with_mirrors_to_destination(
        destination: RangeInclusive<Address>,
        input: impl IntoIterator<Item = (Address, Permissions)>,
    ) -> impl Iterator<Item = Self> {
        input.into_iter().map(move |(base, permissions)| Self::Map {
            permissions,
            range: RangeInclusive::from_start_and_length(base, destination.len()),
            target: MapTarget::Mirror {
                destination: destination.clone(),
            },
        })
    }
}

#[allow(missing_docs)]
#[derive(Debug, Clone, Copy)]
pub struct Permissions {
    pub read: bool,
    pub write: bool,
}

impl Permissions {
    pub const ALL: Self = Permissions {
        read: true,
        write: true,
    };
    pub const READ: Self = Permissions {
        read: true,
        write: false,
    };
    pub const WRITE: Self = Permissions {
        read: false,
        write: true,
    };
}

impl RuntimeHandle {
    /// Obtain a handle to the memory registry
    #[inline]
    pub(crate) fn memory_registry(&self) -> MemoryRegistry<'_> {
        MemoryRegistry::new(self)
    }
}
