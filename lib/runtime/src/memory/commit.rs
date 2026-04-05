use std::ops::RangeInclusive;

use bytes::Bytes;
use fluxemu_range::{ContiguousRange, RangeIntersection};
use itertools::Itertools;

use crate::{
    component::ComponentRegistry,
    memory::{Address, MappingEntry, MemoryMappingTable, PAGE_SIZE, Page, PageEntry, PageTarget},
    path::{ComponentPath, ResourcePath},
};

#[derive(Debug, Clone)]
pub enum MapTarget {
    Component(ComponentPath),
    Memory(ResourcePath),
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
    /// Register a buffer or another item
    Register { path: ResourcePath, buffer: Bytes },
}

#[allow(missing_docs)]
#[derive(Debug, Clone, Copy)]
pub struct Permissions {
    pub read: bool,
    pub write: bool,
}

impl Permissions {
    /// Instance of [Self] where everything is allowed
    pub fn all() -> Self {
        Self {
            read: true,
            write: true,
        }
    }
}

// This function flattens and splits the memory map for faster lookups

impl MemoryMappingTable {
    pub(super) fn commit(
        &mut self,
        registry: ComponentRegistry<'_>,
        resources: &scc::HashMap<ResourcePath, Bytes>,
    ) {
        for (page_index, page) in self.computed_table.iter_mut().enumerate() {
            let base = page_index * PAGE_SIZE;
            let end = base + PAGE_SIZE - 1;
            let page_range = base..=end;

            let mut page_entries: Vec<PageEntry> = self
                .master
                .overlapping(page_range.clone())
                .map(|(range, component)| (range.clone(), component))
                .flat_map(|(source_range, entry)| match entry {
                    MappingEntry::Component(path) => {
                        vec![PageEntry {
                            target: PageTarget::Component {
                                destination_start: *source_range.start(),
                                component: registry.path_to_id(path).unwrap(),
                            },
                            range: source_range,
                        }]
                    }
                    MappingEntry::Mirror {
                        source_base,
                        destination_base,
                    } => {
                        let offset = source_range
                            .start()
                            .checked_sub(*source_base)
                            .expect("mirror source_range.start must be >= source_base");
                        let source_length = source_range.len();

                        let destination_start = destination_base + offset;

                        let assigned_destination_range =
                            RangeInclusive::from_start_and_length(destination_start, source_length);

                        let mut entries: Vec<PageEntry> = self
                            .master
                            .overlapping(assigned_destination_range.clone())
                            .map(|(destination_range, dest_entry)| {
                                let destination_overlap =
                                    assigned_destination_range.intersection(destination_range);

                                let shrink_left = destination_overlap.start()
                                    - assigned_destination_range.start();

                                let shrink_right =
                                    assigned_destination_range.end() - destination_overlap.end();

                                let calculated_source_range = (source_range.start() + shrink_left)
                                    ..=(source_range.end() - shrink_right);

                                match dest_entry {
                                    MappingEntry::Component(path) => PageEntry {
                                        range: calculated_source_range,
                                        target: PageTarget::Component {
                                            destination_start: *destination_overlap.start(),
                                            component: registry.path_to_id(path).unwrap(),
                                        },
                                    },
                                    MappingEntry::Mirror { .. } => {
                                        panic!("Recursive mirrors are not allowed");
                                    }
                                    MappingEntry::Memory(resource_path) => {
                                        let memory = resources.get_sync(resource_path).unwrap();

                                        let buffer_subrange = (destination_overlap.start()
                                            - destination_range.start())
                                            ..=(destination_overlap.end()
                                                - destination_range.start());

                                        let memory = memory.slice(buffer_subrange);
                                        PageEntry {
                                            range: calculated_source_range,
                                            target: PageTarget::Memory(memory),
                                        }
                                    }
                                }
                            })
                            .collect();

                        entries.dedup_by(merge_and_dedup_mirror_entries);

                        entries
                    }
                    MappingEntry::Memory(resource_path) => {
                        let memory = resources.get_sync(resource_path).unwrap();

                        assert_eq!(memory.len(), source_range.len());

                        vec![PageEntry {
                            range: source_range,
                            target: PageTarget::Memory(memory.clone()),
                        }]
                    }
                })
                .sorted_by_key(|entry| *entry.range.start())
                .collect();

            *page = match page_entries.len() {
                0 => None,
                1 => Some(Page::Single(page_entries.remove(0))),
                _ => Some(Page::Multi(page_entries.into())),
            };
        }
    }
}

#[inline]
fn merge_and_dedup_mirror_entries(right: &mut PageEntry, left: &mut PageEntry) -> bool {
    if !left.range.is_adjacent(&right.range) {
        return false;
    }

    match (&mut left.target, &right.target) {
        (
            PageTarget::Component {
                destination_start: destination_start_left,
                component: component_left,
            },
            PageTarget::Component {
                destination_start: destination_start_right,
                component: component_right,
            },
        ) if component_left == component_right
            && *destination_start_right
                == *destination_start_left + (right.range.start() - left.range.start()) =>
        {
            left.range = *left.range.start()..=*right.range.end();

            true
        }
        _ => false,
    }
}
