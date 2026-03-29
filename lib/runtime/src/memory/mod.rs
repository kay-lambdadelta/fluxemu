use std::{fmt::Debug, hash::Hash, ops::RangeInclusive, sync::Arc};

use arc_swap::{ArcSwap, Cache};
use bytes::Bytes;
pub use commit::{MapTarget, MemoryRemappingCommand, Permissions};
use fluxemu_range::RangeIntersection;
use rangemap::RangeInclusiveMap;
use thiserror::Error;

use crate::{
    component::{ComponentHandle, ComponentRegistry},
    path::{ComponentPath, ResourcePath},
};

mod commit;
mod overlapping;
mod read;
mod write;

pub type Address = usize;
const PAGE_SIZE: Address = 0x1000;

/// The main structure representing the devices memory address spaces
#[derive(Debug)]
pub struct AddressSpace {
    width_mask: Address,
    address_space_width: u8,
    id: AddressSpaceId,
    members: Arc<ArcSwap<Members>>,
    resources: scc::HashMap<ResourcePath, Bytes>,
}

impl AddressSpace {
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
            members: Arc::new(ArcSwap::new(Arc::new(Members {
                read: MemoryMappingTable::new(width),
                write: MemoryMappingTable::new(width),
            }))),
            resources: scc::HashMap::default(),
        }
    }

    pub fn create_cache(&self) -> AddressSpaceCache {
        AddressSpaceCache {
            members: Cache::new(self.members.clone()),
        }
    }

    pub(crate) fn remap(
        &self,
        commands: impl IntoIterator<Item = MemoryRemappingCommand>,
        registry: &ComponentRegistry,
    ) {
        let max = 2usize.pow(u32::from(self.address_space_width)) - 1;
        let valid_range = 0..=max;
        let commands: Vec<_> = commands.into_iter().collect();

        self.members.rcu(|members| {
            let mut members = Members::clone(members);

            for command in commands.clone() {
                match command {
                    MemoryRemappingCommand::Map {
                        range,
                        target,
                        permissions,
                    } => {
                        assert!(
                            !valid_range.disjoint(&range),
                            "Range {range:#04x?} is invalid for a address space that ends at \
                             {max:04x?}"
                        );

                        match target {
                            MapTarget::Component(component_path) => {
                                if permissions.read {
                                    members.read.master.insert(
                                        range.clone(),
                                        MappingEntry::Component(component_path.clone()),
                                    );
                                }

                                if permissions.write {
                                    members.write.master.insert(
                                        range.clone(),
                                        MappingEntry::Component(component_path),
                                    );
                                }
                            }
                            MapTarget::Memory(resource_path) => {
                                assert!(
                                    self.resources.contains_sync(&resource_path),
                                    "Resource not found {} (internal resources {:?})",
                                    resource_path,
                                    self.resources
                                );

                                if permissions.read {
                                    members.read.master.insert(
                                        range.clone(),
                                        MappingEntry::Memory(resource_path.clone()),
                                    );
                                }

                                if permissions.write {
                                    members
                                        .write
                                        .master
                                        .insert(range.clone(), MappingEntry::Memory(resource_path));
                                }
                            }
                            MapTarget::Mirror { destination } => {
                                assert!(
                                    !valid_range.disjoint(&destination),
                                    "Range {destination:#04x?} is invalid for a address space \
                                     that ends at {max:04x?}"
                                );

                                if permissions.read {
                                    members.read.master.insert(
                                        range.clone(),
                                        MappingEntry::Mirror {
                                            source_base: *range.start(),
                                            destination_base: *destination.start(),
                                        },
                                    );
                                }

                                if permissions.write {
                                    members.write.master.insert(
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
                            members.read.master.remove(range.clone());
                        }

                        if permissions.write {
                            members.write.master.remove(range.clone());
                        }
                    }
                    MemoryRemappingCommand::Register { path, buffer } => {
                        self.resources.insert_sync(path, buffer).unwrap();
                    }
                }
            }

            members.read.commit(registry, &self.resources);
            members.write.commit(registry, &self.resources);

            members
        });
    }

    pub fn id(&self) -> AddressSpaceId {
        self.id
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MappingEntry {
    Component(ComponentPath),
    Mirror {
        source_base: Address,
        destination_base: Address,
    },
    Memory(ResourcePath),
}

#[derive(Debug, Clone)]
pub enum PageTarget {
    Component {
        mirror_start: Option<Address>,
        component: ComponentHandle,
    },
    Memory(Bytes),
}

#[derive(Debug, Clone)]
pub struct PageEntry {
    /// Full, uncropped relevant range
    pub range: RangeInclusive<Address>,
    pub target: PageTarget,
}

#[derive(Debug, Clone)]
pub enum Page {
    Single(PageEntry),
    Multi(Box<[PageEntry]>),
}

#[derive(Debug, Clone)]
pub struct MemoryMappingTable {
    master: RangeInclusiveMap<Address, MappingEntry>,
    computed_table: Vec<Option<Page>>,
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

#[derive(Debug, Copy, Clone, Eq, PartialEq, PartialOrd, Ord, Hash)]
/// Identifier for a address space
pub struct AddressSpaceId(pub(crate) u16);

#[derive(Debug, Clone)]
pub struct Members {
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

#[derive(Error)]
#[error("Memory operation failed: {0:#x?}")]
/// Wrapper around the error type in order to specify ranges
pub struct MemoryError(pub RangeInclusiveMap<Address, MemoryErrorType>);

impl Debug for MemoryError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("MemoryError")
            .field(&format_args!("{:x?}", self.0))
            .finish()
    }
}

#[derive(Debug)]
pub struct AddressSpaceCache {
    members: Cache<Arc<ArcSwap<Members>>, Arc<Members>>,
}
