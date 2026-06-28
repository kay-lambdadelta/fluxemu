use std::{
    any::TypeId,
    ops::RangeInclusive,
    sync::{Arc, atomic::Ordering},
};

use fluxemu_range::{ContiguousRange, RangeIntersection};
use rangemap::{RangeInclusiveMap, RangeInclusiveSet};
use sdd::{Guard, Owned, Tag};

use crate::{
    component::ComponentRegistry,
    memory::{
        Address, AddressSpaceData, MAX_MIRROR_DEPTH, MapTarget, MasterTableEntry, MasterTables,
        Members, MemoryRemappingCommand, PAGE_SIZE, PageTable, PageTableEntry, PageTableTarget,
        Permissions, component::Memory,
    },
    scheduler::Period,
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

impl PageTable {
    // NOTE:
    //
    // ANY changes to this functions should involve a FULL review of the functions in `ops.rs`
    //
    // It relies on the current behavior of this function quite a bit

    #[inline]
    fn commit(
        &mut self,
        previous_table: &Self,
        master: &RangeInclusiveMap<Address, MasterTableEntry>,
        dirty: &RangeInclusiveSet<Address>,
        registry: ComponentRegistry<'_>,
    ) {
        for (page_index, page) in self.0.iter_mut().enumerate() {
            let page_address_range =
                RangeInclusive::from_start_and_length(page_index * PAGE_SIZE, PAGE_SIZE);

            if dirty.overlaps(&page_address_range) {
                let mut page_contents = Vec::default();

                // Compile master table entries into this page
                for (source_range, entry) in master.overlapping(page_address_range.clone()) {
                    match entry {
                        MasterTableEntry::Component(path) => {
                            page_contents.push(PageTableEntry {
                                target: PageTableTarget::Component {
                                    destination_start: *source_range.start(),
                                    component_id: registry.path_to_id(path).unwrap(),
                                    is_standard_memory: registry.typeid(path).unwrap()
                                        == TypeId::of::<Memory>(),
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
                                registry,
                                source_range.clone(),
                                assigned_destination_range,
                                &mut page_contents,
                                0,
                            );
                        }
                        MasterTableEntry::Buffer(memory) => {
                            // Validate the buffer subrange matches the range its being put into
                            assert_eq!(memory.len(), source_range.len());

                            page_contents.push(PageTableEntry {
                                range: source_range.clone().into(),
                                target: PageTableTarget::Memory(memory.clone()),
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
                            destination_start: destination_start_a,
                            component_id: component_id_a,
                            ..
                        },
                        PageTableTarget::Component {
                            destination_start: destination_start_b,
                            component_id: component_id_b,
                            ..
                        },
                    ) => {
                        // Same component check
                        if component_id_a != component_id_b {
                            return false;
                        }

                        // Virtual contiguous check
                        if !a.range.is_adjacent(&b.range) {
                            return false;
                        }

                        // Physical contiguous check
                        if *destination_start_a != destination_start_b + b.range.len() {
                            return false;
                        }

                        // Merge them
                        b.range = (b.range.start..=a.range.last).into();

                        true
                    }
                    _ => false,
                });

                *page = Arc::from(page_contents);
            } else {
                *page = previous_table.0[page_index].clone();
            }
        }
    }

    #[inline]
    fn resolve_mirror_target(
        master: &RangeInclusiveMap<Address, MasterTableEntry>,
        registry: ComponentRegistry<'_>,
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
                            destination_start: *destination_overlap.start(),
                            component_id: registry.path_to_id(path).unwrap(),
                            is_standard_memory: registry.typeid(path).unwrap()
                                == TypeId::of::<Memory>(),
                        },
                    });
                }
                MasterTableEntry::Buffer(memory) => {
                    let buffer_subrange = (destination_overlap.start() - destination_range.start())
                        ..=(destination_overlap.end() - destination_range.start());

                    let memory = memory.slice(buffer_subrange.clone());

                    assert_eq!(
                        memory.len(),
                        buffer_subrange.len(),
                        "Buffers has to be the same length as the range they are being mapped into"
                    );

                    page.push(PageTableEntry {
                        range: calculated_source_range.into(),
                        target: PageTableTarget::Memory(memory),
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
                        registry,
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
    fn mirror_dirtying_pass(
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

impl AddressSpaceData {
    #[inline]
    pub fn remap(
        &self,
        timestamp: Period,
        registry: ComponentRegistry<'_>,
        guard: &Guard,
        commands: impl IntoIterator<Item = MemoryRemappingCommand>,
    ) {
        let max = 2usize.pow(u32::from(self.address_space_width)) - 1;
        let valid_range = 0..=max;
        let commands: Vec<_> = commands.into_iter().collect();

        let mut dirty_read = RangeInclusiveSet::new();
        let mut dirty_write = RangeInclusiveSet::new();

        // We are also using this as a write serializer
        let mut master_tables_guard = self.master.lock().unwrap();
        let MasterTables {
            read: master_read,
            write: master_write,
        } = &mut *master_tables_guard;

        let current = self.members.load(Ordering::Acquire, guard);
        let current_members = current.as_ref().unwrap();

        let mut read_table =
            PageTable(vec![Default::default(); current_members.read.0.len()].into_boxed_slice());
        let mut write_table =
            PageTable(vec![Default::default(); current_members.write.0.len()].into_boxed_slice());

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
                                master_read.insert(
                                    range.clone(),
                                    MasterTableEntry::Component(component_path.clone()),
                                );
                            }

                            if permissions.write {
                                master_write.insert(
                                    range.clone(),
                                    MasterTableEntry::Component(component_path),
                                );
                            }
                        }
                        MapTarget::Buffer(bytes) => {
                            if permissions.read {
                                master_read
                                    .insert(range.clone(), MasterTableEntry::Buffer(bytes.clone()));
                            }

                            if permissions.write {
                                unreachable!("Buffers are read only");
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
                                master_read.insert(
                                    range.clone(),
                                    MasterTableEntry::Mirror {
                                        source_base: *range.start(),
                                        destination_base: *destination.start(),
                                    },
                                );
                            }

                            if permissions.write {
                                master_write.insert(
                                    range.clone(),
                                    MasterTableEntry::Mirror {
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
                        master_read.remove(range.clone());
                        dirty_read.insert(range.clone());
                    }

                    if permissions.write {
                        master_write.remove(range.clone());
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

        read_table.mirror_dirtying_pass(master_read, &mut dirty_read);
        write_table.mirror_dirtying_pass(master_write, &mut dirty_write);

        read_table.commit(&current_members.read, master_read, &dirty_read, registry);
        write_table.commit(&current_members.write, master_write, &dirty_write, registry);

        // Make sure owning components in the old map get synchronized to this timestamp before the remapping is actually applied
        //
        // This is a "fence"
        for (dirty, master) in [(dirty_read, &master_read), (dirty_write, &master_write)] {
            for dirty_entry in dirty {
                for component_path in
                    master
                        .overlapping(dirty_entry)
                        .filter_map(|(_, mapping_entry)| match mapping_entry {
                            MasterTableEntry::Component(path) => Some(path),
                            _ => None,
                        })
                {
                    registry
                        .interact_dyn(component_path, timestamp, |_| {})
                        .unwrap();
                }
            }
        }

        let members = Members {
            read: read_table,
            write: write_table,
        };

        let _ = self
            .members
            .swap((Some(Owned::new(members)), Tag::None), Ordering::AcqRel);
    }
}
