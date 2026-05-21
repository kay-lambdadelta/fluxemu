use std::{
    fmt::Debug,
    hash::Hash,
    ops::RangeInclusive,
    sync::{Mutex, atomic::Ordering},
};

use bytes::Bytes;
use fluxemu_range::ContiguousRange;
use rangemap::{RangeInclusiveMap, RangeInclusiveSet};
use sdd::{AtomicOwned, Guard, Owned, Tag};
use smallvec::SmallVec;
use thiserror::Error;

use crate::{
    RuntimeApi,
    component::{ComponentId, ComponentRegistry},
    path::ComponentPath,
    scheduler::Period,
};

mod commit;
pub mod component;
mod read;
#[cfg(test)]
mod tests;
mod write;

pub type Address = usize;
const PAGE_SIZE: Address = 0x1000;

/// The main structure representing the devices memory address spaces
#[derive(Debug)]
pub struct AddressSpace<'a> {
    registry: ComponentRegistry<'a>,
    data: &'a AddressSpaceData,
    guard: Guard,
}

impl<'a> AddressSpace<'a> {
    pub(crate) fn new(runtime: &'a RuntimeApi, data: &'a AddressSpaceData) -> Self {
        Self {
            registry: runtime.registry(),
            data,
            guard: Guard::new(),
        }
    }

    /// Change the memory mapping based upon the command list given
    ///
    /// Note that:
    ///
    /// - Mapping changes are ADDITIVE, they apply on top of existing mappings
    /// - Within a given command set this is an atomic operation, however it does not block accesses to address space methods while it
    ///   is completing
    /// - If two remappings from different threads are done at the same time, its unspecified which one "wins"
    /// - As of the current implementation, remapping is rather expensive. Use sparingly or improve the committing code
    pub fn remap(
        &self,
        timestamp: Period,
        commands: impl IntoIterator<Item = MemoryRemappingCommand>,
    ) {
        self.data.remap(timestamp, self.registry, commands);
    }
}

impl<'a> AddressSpace<'a> {
    pub fn id(&self) -> AddressSpaceId {
        self.data.id
    }
}

#[derive(Debug, Clone)]
enum MappingEntry {
    Component(ComponentPath),
    Mirror {
        source_base: Address,
        destination_base: Address,
    },
    Buffer(Bytes),
}

impl PartialEq for MappingEntry {
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

impl Eq for MappingEntry {}

#[derive(Clone, Debug)]
enum PageTarget {
    Memory(Bytes),
    Component {
        destination_start: Address,
        component_id: ComponentId,
        is_standard_memory: bool,
    },
}

#[derive(Debug, Clone)]
struct PageEntry {
    /// Full, uncropped relevant range
    pub range: RangeInclusive<Address>,
    pub target: PageTarget,
}

#[derive(Debug, Clone)]
struct MemoryMappingTable {
    master: RangeInclusiveMap<Address, MappingEntry>,
    computed_table: Vec<SmallVec<PageEntry, 1>>,
}

impl MemoryMappingTable {
    pub fn new(address_space_width: u8) -> Self {
        let addr_space_size = 2usize.pow(u32::from(address_space_width));
        let total_pages = addr_space_size.div_ceil(PAGE_SIZE);

        Self {
            master: RangeInclusiveMap::new(),
            computed_table: vec![Default::default(); total_pages],
        }
    }
}

/// Identifier for a address space
#[derive(Debug, Copy, Clone, Eq, PartialEq, PartialOrd, Ord, Hash)]
pub struct AddressSpaceId(pub(crate) u16);

#[derive(Debug, Clone)]
struct Members {
    pub read: MemoryMappingTable,
    pub write: MemoryMappingTable,
}

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
/// Why a read operation failed
pub enum MemoryErrorType {
    /// Access was denied
    Denied,
    /// Nothing is mapped there
    OutOfBus,
    /// It would be impossible to view this memory without a state change
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

#[derive(Debug)]
pub(crate) struct AddressSpaceData {
    width_mask: Address,
    address_space_width: u8,
    id: AddressSpaceId,
    members: AtomicOwned<Members>,
    write_lock: Mutex<()>,
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
                read: MemoryMappingTable::new(width),
                write: MemoryMappingTable::new(width),
            }),
            write_lock: Mutex::default(),
        }
    }

    pub fn remap(
        &self,
        timestamp: Period,
        registry: ComponentRegistry<'_>,
        commands: impl IntoIterator<Item = MemoryRemappingCommand>,
    ) {
        let max = 2usize.pow(u32::from(self.address_space_width)) - 1;
        let valid_range = 0..=max;
        let commands: Vec<_> = commands.into_iter().collect();

        let mut dirty_read = RangeInclusiveSet::new();
        let mut dirty_write = RangeInclusiveSet::new();

        let load_guard = Guard::new();
        let _write_lock_guard = self.write_lock.lock().unwrap();

        let current = self.members.load(Ordering::Acquire, &load_guard);
        let current_members = current.as_ref().unwrap();
        let mut new_members = current_members.clone();

        for command in commands {
            match command {
                MemoryRemappingCommand::Map {
                    range,
                    target,
                    permissions,
                } => {
                    assert!(
                        valid_range.contains(range.start()) && valid_range.contains(range.end()),
                        "Range {range:#04x?} is invalid for a address space that ends at \
                         {max:04x?}"
                    );

                    if permissions.read {
                        dirty_read.insert(range.clone());
                    }

                    if permissions.write {
                        dirty_write.insert(range.clone());
                    }

                    match target {
                        MapTarget::Component(component_path) => {
                            if permissions.read {
                                new_members.read.master.insert(
                                    range.clone(),
                                    MappingEntry::Component(component_path.clone()),
                                );
                            }

                            if permissions.write {
                                new_members
                                    .write
                                    .master
                                    .insert(range.clone(), MappingEntry::Component(component_path));
                            }
                        }
                        MapTarget::Buffer(bytes) => {
                            if permissions.read {
                                new_members
                                    .read
                                    .master
                                    .insert(range.clone(), MappingEntry::Buffer(bytes.clone()));
                            }

                            if permissions.write {
                                new_members
                                    .write
                                    .master
                                    .insert(range.clone(), MappingEntry::Buffer(bytes));
                            }
                        }
                        MapTarget::Mirror { destination } => {
                            assert!(
                                valid_range.contains(range.start())
                                    && valid_range.contains(range.end()),
                                "Range {destination:#04x?} is invalid for a address space that \
                                 ends at {max:04x?}"
                            );

                            assert_eq!(
                                range.len(),
                                destination.len(),
                                "Mirror source and destination ranges must have the same length"
                            );

                            if permissions.read {
                                new_members.read.master.insert(
                                    range.clone(),
                                    MappingEntry::Mirror {
                                        source_base: *range.start(),
                                        destination_base: *destination.start(),
                                    },
                                );
                            }

                            if permissions.write {
                                new_members.write.master.insert(
                                    range.clone(),
                                    MappingEntry::Mirror {
                                        source_base: *range.start(),
                                        destination_base: *destination.start(),
                                    },
                                );
                            }
                        }
                    }
                }
                MemoryRemappingCommand::Unmap { range, permissions } => {
                    if permissions.read {
                        new_members.read.master.remove(range.clone());
                        dirty_read.insert(range.clone());
                    }

                    if permissions.write {
                        new_members.write.master.remove(range.clone());
                        dirty_write.insert(range.clone());
                    }
                }
                MemoryRemappingCommand::RebaseComponent { component, base } => {
                    registry.interact_dyn(&component, timestamp, |component| {
                        component.memory_rebase(base);
                    });
                }
            }
        }

        new_members.read.mirror_dirtying_pass(&mut dirty_read);
        new_members.read.commit(&dirty_read, registry);
        new_members.write.mirror_dirtying_pass(&mut dirty_write);
        new_members.write.commit(&dirty_write, registry);

        // Make sure owning components in the old map get synchronized to this timestamp before the remapping is actually applied
        //
        // This is a "fence"
        for (dirty, table) in [
            (dirty_read, &current_members.read),
            (dirty_write, &current_members.write),
        ] {
            for dirty_entry in dirty {
                for component_path in
                    table
                        .master
                        .overlapping(dirty_entry)
                        .filter_map(|(_, mapping_entry)| match mapping_entry {
                            MappingEntry::Component(path) => Some(path),
                            _ => None,
                        })
                {
                    registry
                        .interact_dyn(component_path, timestamp, |_| {})
                        .unwrap();
                }
            }
        }

        drop(load_guard);

        let _ = self
            .members
            .swap((Some(Owned::new(new_members)), Tag::None), Ordering::AcqRel);
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
