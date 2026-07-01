use std::{
    fmt::Debug,
    hash::Hash,
    ops::RangeInclusive,
    sync::{Arc, Mutex, atomic::Ordering},
};

use bytes::Bytes;
use rangemap::RangeInclusiveMap;
use sdd::{AtomicOwned, Guard};
use thiserror::Error;

use crate::{
    component::{ComponentId, ComponentRegistry},
    path::ComponentPath,
    scheduler::Period,
};

pub mod component;
mod ops;
mod remap;
#[cfg(test)]
mod tests;

pub type Address = usize;
const PAGE_SIZE: Address = 0x1000;
const MAX_MIRROR_DEPTH: usize = 4;

/// Handle to a address space specificed at machine registration time
///
/// This is the primary interface for accessing memory in the runtime
#[derive(Debug)]
pub struct AddressSpace<'a> {
    registry: ComponentRegistry<'a>,
    data: &'a AddressSpaceData,
    guard: Guard,
}

impl<'a> AddressSpace<'a> {
    #[inline]
    pub(crate) fn new(registry: ComponentRegistry<'a>, data: &'a AddressSpaceData) -> Self {
        Self {
            registry,
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
        &self,
        timestamp: Period,
        commands: impl IntoIterator<Item = MemoryRemappingCommand>,
    ) {
        // Some programs who do extremely frequent remappings will inflate ram with old copies of the mapping table
        //
        // This informs `sdd` that cleanup should happen sooner rather than later
        self.guard.accelerate();

        self.data
            .remap(timestamp, &self.registry, &self.guard, commands);
    }
}

impl<'a> AddressSpace<'a> {
    pub fn id(&self) -> AddressSpaceId {
        self.data.id
    }
}

#[derive(Clone, Debug)]
enum PageTableTarget {
    Memory(Bytes),
    Component {
        destination_start: Address,
        component_id: ComponentId,
        is_standard_memory: bool,
    },
}

#[derive(Debug, Clone)]
struct PageTableEntry {
    /// Full, uncropped relevant range
    pub range: std::range::RangeInclusive<Address>,
    pub target: PageTableTarget,
}

#[derive(Debug)]
struct PageTable(Box<[Arc<[PageTableEntry]>]>);

impl PageTable {
    pub fn new(address_space_width: u8) -> Self {
        let addr_space_size = 2usize.pow(u32::from(address_space_width));
        let total_pages = addr_space_size.div_ceil(PAGE_SIZE);

        Self(vec![Default::default(); total_pages].into_boxed_slice())
    }
}

/// Identifier for a address space
#[derive(Debug, Copy, Clone, Eq, PartialEq, PartialOrd, Ord, Hash)]
pub struct AddressSpaceId(pub(crate) u16);

#[derive(Debug)]
struct Members {
    pub read: PageTable,
    pub write: PageTable,
}

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
pub struct MemoryError(pub Box<[(RangeInclusive<Address>, MemoryErrorType)]>);

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
    Component(ComponentPath),
    Mirror {
        source_base: Address,
        destination_base: Address,
    },
    Buffer(Bytes),
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
            // Never coalesce buffer entries
            (Self::Buffer(_), Self::Buffer(_)) => false,
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
    members: AtomicOwned<Members>,
    master: Mutex<MasterTables>,
}

impl AddressSpaceData {
    pub(crate) fn new(id: AddressSpaceId, width: u8) -> Self {
        assert!(
            width as usize <= usize::BITS as usize,
            "width exceeds usize::BITS"
        );

        let width_mask: usize = (1 << width) - 1;

        Self {
            id,
            width_mask,
            address_space_width: width,
            members: AtomicOwned::new(Members {
                read: PageTable::new(width),
                write: PageTable::new(width),
            }),
            master: Mutex::default(),
        }
    }

    #[inline]
    fn get_members<'a>(&'a self, guard: &'a Guard) -> &'a Members {
        // SAFETY: We never set an null members mapping, and we don't set any tag bits
        unsafe {
            self.members
                .load(Ordering::Acquire, guard)
                .as_ref_unchecked()
                .unwrap_unchecked()
        }
    }
}

#[derive(Debug, Clone)]
pub enum MapTarget {
    Component(ComponentPath),
    Buffer(Bytes),
    Mirror {
        destination: RangeInclusive<Address>,
    },
}

/// Command for how the memory access table should modify the memory map
#[allow(missing_docs)]
#[derive(Debug, Clone)]
pub enum MemoryRemappingCommand {
    /// Add a target to the memory map, or add a map to an existing one
    Map {
        range: RangeInclusive<Address>,
        target: MapTarget,
        permissions: Permissions,
    },
    /// Clear a memory range
    Unmap {
        range: RangeInclusive<Address>,
        permissions: Permissions,
    },
    /// Notify the component that its base must be changed to an address to function correctly
    RebaseComponent {
        component: ComponentPath,
        base: Address,
    },
}

#[allow(missing_docs)]
#[derive(Debug, Clone, Copy)]
pub struct Permissions {
    pub read: bool,
    pub write: bool,
}
