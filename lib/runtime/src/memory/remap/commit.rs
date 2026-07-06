use std::{ops::RangeInclusive, sync::Arc};

use fluxemu_range::{ContiguousRange, RangeIntersection};
use rangemap::{RangeInclusiveMap, RangeInclusiveSet};

use crate::{
    component::ComponentRegistry,
    memory::{
        Address, CHUNK_SIZE, MAX_MIRROR_DEPTH, MasterTableEntry, PageTable, PageTableEntry,
        PageTableTarget, registry::MemoryRegistry,
    },
};

impl PageTable {
    // NOTE:
    //
    // ANY changes to this functions should involve a FULL review of the functions in `ops.rs`
    //
    // It relies on the current behavior of this function quite a bit

    #[inline]
    pub fn commit(
        &mut self,
        previous_table: &Self,
        master: &RangeInclusiveMap<Address, MasterTableEntry>,
        dirty: &RangeInclusiveSet<Address>,
        component_registry: &mut ComponentRegistry<'_>,
        memory_registry: &mut MemoryRegistry<'_>,
    ) {
        for page_index in 0..self.0.len() {
            let page_address_range =
                RangeInclusive::from_start_and_length(page_index * CHUNK_SIZE, CHUNK_SIZE);

            self.0[page_index] = if dirty.overlaps(&page_address_range) {
                let mut page_contents = Vec::default();

                // Compile master table entries into this page
                for (source_range, entry) in master.overlapping(page_address_range.clone()) {
                    match entry {
                        MasterTableEntry::Component(path) => {
                            page_contents.push(PageTableEntry {
                                target: PageTableTarget::Component {
                                    offset: *source_range.start(),
                                    id: component_registry.id_for_path(path).unwrap(),
                                },
                                range: source_range.clone().into(),
                            });
                        }
                        MasterTableEntry::Mirror {
                            source_base,
                            destination_base,
                        } => {
                            let offset = source_range
                                .start()
                                .checked_sub(*source_base)
                                .expect("mirror source_range.start must be >= source_base");

                            let destination_start = destination_base + offset;
                            let assigned_destination_range = RangeInclusive::from_start_and_length(
                                destination_start,
                                source_range.len(),
                            );

                            Self::resolve_mirror_target(
                                master,
                                component_registry,
                                memory_registry,
                                source_range.clone(),
                                assigned_destination_range,
                                &mut page_contents,
                                0,
                            );
                        }
                        MasterTableEntry::ImmutableMemory(memory) => {
                            // Validate the buffer subrange matches the range its being put into
                            assert_eq!(memory.len(), source_range.len());

                            page_contents.push(PageTableEntry {
                                range: source_range.clone().into(),
                                target: PageTableTarget::ImmutableMemory(memory.clone()),
                            });
                        }
                        MasterTableEntry::Memory {
                            path,
                            source_base,
                            region_base,
                            length,
                        } => {
                            let region_offset = source_range.start() - source_base;

                            assert!(
                                region_offset + source_range.len() <= *length,
                                "Mapping does not fit within memory region"
                            );

                            page_contents.push(PageTableEntry {
                                range: source_range.clone().into(),
                                target: PageTableTarget::Memory {
                                    offset: region_base + region_offset,
                                    id: memory_registry.id_for_path(path).unwrap(),
                                },
                            });
                        }
                    }
                }

                // Make sure what we put in is sorted
                page_contents.sort_by_key(|entry| entry.range.start);

                // Deduplicate
                page_contents.dedup_by(|a, b| match (&a.target, &b.target) {
                    (
                        PageTableTarget::Component {
                            offset: offset_a,
                            id: id_a,
                        },
                        PageTableTarget::Component {
                            offset: offset_b,
                            id: id_b,
                        },
                    ) => {
                        // Same component check
                        if id_a != id_b {
                            return false;
                        }

                        // Virtual contiguous check
                        if !a.range.is_adjacent(&b.range) {
                            return false;
                        }

                        // Physical contiguous check
                        if *offset_a != offset_b + b.range.len() {
                            return false;
                        }

                        // Merge them
                        b.range = (b.range.start..=a.range.last).into();

                        true
                    }
                    (
                        PageTableTarget::Memory {
                            offset: offset_a,
                            id: id_a,
                        },
                        PageTableTarget::Memory {
                            offset: offset_b,
                            id: id_b,
                        },
                    ) => {
                        if id_a != id_b {
                            return false;
                        }
                        if !a.range.is_adjacent(&b.range) {
                            return false;
                        }
                        if *offset_a != offset_b + b.range.len() {
                            return false;
                        }
                        b.range = (b.range.start..=a.range.last).into();

                        true
                    }
                    _ => false,
                });

                let previous_page = page_index.checked_sub(1).map(|index| &self.0[index]);

                if let Some(previous_page) = previous_page
                    && pages_have_same_mapping(previous_page, &page_contents)
                {
                    previous_page.clone()
                } else {
                    Arc::from(page_contents)
                }
            } else {
                previous_table.0[page_index].clone()
            };
        }
    }

    #[inline]
    fn resolve_mirror_target(
        master: &RangeInclusiveMap<Address, MasterTableEntry>,
        component_registry: &ComponentRegistry,
        memory_registry: &MemoryRegistry,
        source_range: RangeInclusive<Address>,
        target_range: RangeInclusive<Address>,
        page: &mut Vec<PageTableEntry>,
        depth: usize,
    ) {
        if depth > MAX_MIRROR_DEPTH {
            panic!(
                "Max mirror depth hit at {} with source range {:?} and target range {:?}",
                depth, source_range, target_range
            );
        }

        for (destination_range, destination_entry) in master.overlapping(target_range.clone()) {
            let destination_overlap = target_range.intersection(destination_range);

            let shrink_left = destination_overlap.start() - target_range.start();
            let shrink_right = target_range.end() - destination_overlap.end();

            let calculated_source_range =
                (source_range.start() + shrink_left)..=(source_range.end() - shrink_right);

            match destination_entry {
                MasterTableEntry::Component(path) => {
                    page.push(PageTableEntry {
                        range: calculated_source_range.into(),
                        target: PageTableTarget::Component {
                            offset: *destination_overlap.start(),
                            id: component_registry.id_for_path(path).unwrap(),
                        },
                    });
                }
                MasterTableEntry::ImmutableMemory(memory) => {
                    let buffer_subrange = (destination_overlap.start() - destination_range.start())
                        ..=(destination_overlap.end() - destination_range.start());

                    let memory = memory.slice(buffer_subrange.clone());

                    assert_eq!(
                        memory.len(),
                        buffer_subrange.len(),
                        "Buffers have to be the same length as the range they are being mapped \
                         into"
                    );

                    page.push(PageTableEntry {
                        range: calculated_source_range.into(),
                        target: PageTableTarget::ImmutableMemory(memory),
                    });
                }
                MasterTableEntry::Memory {
                    path,
                    source_base,
                    region_base,
                    length,
                } => {
                    let region_offset = destination_overlap.start() - source_base;

                    assert!(
                        region_offset + destination_overlap.len() <= *length,
                        "Mirror target does not fit within memory region"
                    );

                    page.push(PageTableEntry {
                        range: calculated_source_range.into(),
                        target: PageTableTarget::Memory {
                            offset: region_base + region_offset,
                            id: memory_registry.id_for_path(path).unwrap(),
                        },
                    });
                }
                MasterTableEntry::Mirror {
                    source_base,
                    destination_base,
                } => {
                    let offset = destination_overlap
                        .start()
                        .checked_sub(*source_base)
                        .expect("mirror destination_overlap.start must be >= source_base");

                    let next_destination_start = destination_base + offset;
                    let next_target_range = RangeInclusive::from_start_and_length(
                        next_destination_start,
                        destination_overlap.len(),
                    );

                    Self::resolve_mirror_target(
                        master,
                        component_registry,
                        memory_registry,
                        calculated_source_range,
                        next_target_range,
                        page,
                        depth + 1,
                    );
                }
            }
        }
    }

    #[inline]
    pub fn mirror_dirtying_pass(
        &mut self,
        master: &RangeInclusiveMap<Address, MasterTableEntry>,
        dirty: &mut RangeInclusiveSet<usize>,
    ) {
        loop {
            let mut changed = false;

            for (master_region, mapping_entry) in master.iter() {
                if let MasterTableEntry::Mirror {
                    source_base,
                    destination_base,
                } = mapping_entry
                {
                    let destination_range = RangeInclusive::from_start_and_length(
                        *destination_base,
                        master_region.len(),
                    );

                    let source_range =
                        RangeInclusive::from_start_and_length(*source_base, master_region.len());

                    if dirty.overlaps(&destination_range) && !dirty.overlaps(&source_range) {
                        dirty.insert(source_range);
                        changed = true;
                    }
                }
            }

            if !changed {
                break;
            }
        }
    }
}

impl PageTableTarget {
    #[inline]
    fn same_mapping(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::ImmutableMemory(a), Self::ImmutableMemory(b)) => {
                a.as_ptr() == b.as_ptr() && a.len() == b.len()
            }
            (
                Self::Component {
                    offset: offset_a,
                    id: id_a,
                },
                Self::Component {
                    offset: offset_b,
                    id: id_b,
                },
            ) => offset_a == offset_b && id_a == id_b,
            (
                Self::Memory {
                    offset: offset_a,
                    id: id_a,
                },
                Self::Memory {
                    offset: offset_b,
                    id: id_b,
                },
            ) => offset_a == offset_b && id_a == id_b,
            _ => false,
        }
    }
}

#[inline]
fn pages_have_same_mapping(a: &[PageTableEntry], b: &[PageTableEntry]) -> bool {
    a.len() == b.len()
        && a.iter()
            .zip(b)
            .all(|(a, b)| a.range == b.range && a.target.same_mapping(&b.target))
}
