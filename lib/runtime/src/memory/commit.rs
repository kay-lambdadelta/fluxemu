use std::ops::RangeInclusive;

use fluxemu_range::{ContiguousRange, RangeIntersection};
use itertools::Itertools;
use rangemap::RangeInclusiveSet;

use crate::{
    component::ComponentRegistry,
    memory::{
        Address, MappingEntry, MemoryMappingTable, PAGE_SIZE, Page, PageEntry, PageTarget,
        Permissions,
    },
};

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

impl MemoryMappingTable {
    /// This function flattens and splits the memory map for faster lookups
    pub(super) fn commit(
        &mut self,
        dirty: RangeInclusiveSet<Address>,
        registry: ComponentRegistry<'_>,
    ) {
        for region in dirty {
            let region_page_range = region.start() / PAGE_SIZE..=region.end() / PAGE_SIZE;

            // Touch the pages in the region that are dirty
            for (page_index, page) in self.computed_table[region_page_range.clone()]
                .iter_mut()
                .enumerate()
            {
                let page_index = page_index + region_page_range.start();
                let page_address_range =
                    RangeInclusive::from_start_and_length(page_index * PAGE_SIZE, PAGE_SIZE);

                // Pull out all relevant entries from the overlapping page range in the master table
                let mut page_entries: Vec<PageEntry> = self
                    .master
                    .overlapping(page_address_range.clone())
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

                            let assigned_destination_range = RangeInclusive::from_start_and_length(
                                destination_start,
                                source_length,
                            );

                            let mut entries: Vec<PageEntry> = self
                                .master
                                .overlapping(assigned_destination_range.clone())
                                .map(|(destination_range, dest_entry)| {
                                    let destination_overlap =
                                        assigned_destination_range.intersection(destination_range);

                                    let shrink_left = destination_overlap.start()
                                        - assigned_destination_range.start();

                                    let shrink_right = assigned_destination_range.end()
                                        - destination_overlap.end();

                                    let calculated_source_range = (source_range.start()
                                        + shrink_left)
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
                                        MappingEntry::Buffer(memory) => {
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
                        MappingEntry::Buffer(memory) => {
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
    pub fn mirror_dirtying_pass(&mut self, dirty: &mut RangeInclusiveSet<usize>) {
        for (master_region, mapping_entry) in self.master.iter() {
            if let MappingEntry::Mirror {
                source_base,
                destination_base,
            } = mapping_entry
            {
                let destination_range =
                    RangeInclusive::from_start_and_length(*destination_base, master_region.len());

                if dirty.overlaps(&destination_range) {
                    let source_range =
                        RangeInclusive::from_start_and_length(*source_base, master_region.len());

                    dirty.insert(source_range);
                }
            }
        }
    }
}

#[inline]
fn merge_and_dedup_mirror_entries(left: &mut PageEntry, right: &mut PageEntry) -> bool {
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
