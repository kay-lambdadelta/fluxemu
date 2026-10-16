use std::{ops::RangeInclusive, sync::atomic::Ordering};

use fluxemu_math::range::ContiguousRange;
use rangemap::{RangeInclusiveMap, RangeInclusiveSet};
use sdd::{Guard, Owned, Tag};

use crate::{
    RuntimeHandle,
    memory::{
        Address, AddressSpaceData, MapTarget, MasterTableEntry, MasterTables, MemoryMapCommand,
    },
    scheduler::Period,
};

mod commit;

impl AddressSpaceData {
    #[inline]
    pub fn remap(
        &self,
        _timestamp: Period,
        guard: &Guard,
        runtime: &RuntimeHandle,
        commands: impl IntoIterator<Item = MemoryMapCommand>,
    ) {
        let max = 2usize.pow(u32::from(self.address_space_width)) - 1;
        let valid_range = 0..=max;

        let mut dirty_read = RangeInclusiveSet::new();
        let mut dirty_write = RangeInclusiveSet::new();

        // We are also using this as a write serializer
        let mut master_tables_guard = self.master.lock().unwrap();
        let MasterTables {
            read: master_read,
            write: master_write,
        } = &mut *master_tables_guard;

        let current_read = self.read_table.load(Ordering::Acquire, guard);
        let current_read_page_table = current_read.as_ref().unwrap();

        let current_write = self.write_table.load(Ordering::Acquire, guard);
        let current_write_page_table = current_write.as_ref().unwrap();

        for command in commands {
            match command {
                MemoryMapCommand::Map {
                    range,
                    target,
                    permissions,
                } => {
                    assert!(
                        valid_range.contains(range.start()) && valid_range.contains(range.end()),
                        "Range {range:#04x?} is invalid for a address space that ends at \
                         {max:04x?}"
                    );

                    let entry = match target {
                        MapTarget::Component(component_path) => {
                            MasterTableEntry::Component(component_path)
                        }
                        MapTarget::ImmutableMemory(bytes) => {
                            assert!(!permissions.write, "Buffers are read only");

                            MasterTableEntry::ImmutableMemory(bytes)
                        }
                        MapTarget::Mirror { destination } => {
                            assert!(
                                valid_range.contains(destination.start())
                                    && valid_range.contains(destination.end()),
                                "Range {destination:#04x?} is invalid for a address space that \
                                 ends at {max:04x?}"
                            );

                            assert_eq!(
                                range.len(),
                                destination.len(),
                                "Mirror source and destination ranges must have the same length"
                            );

                            MasterTableEntry::Mirror {
                                source_base: *range.start(),
                                destination_base: *destination.start(),
                            }
                        }
                        MapTarget::Memory { path, subrange } => {
                            let region_size = runtime
                                .memory_registry()
                                .region_size(&path)
                                .unwrap_or_else(|| panic!("Memory region {path} does not exist"));

                            let subrange = subrange.unwrap_or_else(|| {
                                RangeInclusive::from_start_and_length(0, region_size)
                            });

                            assert!(range.len() <= subrange.len());

                            MasterTableEntry::Memory {
                                path,
                                source_base: *range.start(),
                                region_base: *subrange.start(),
                                length: range.len(),
                            }
                        }
                    };

                    if permissions.read {
                        dirty_read.insert(range.clone());
                        master_read.insert(range.clone(), entry.clone());
                    }

                    if permissions.write {
                        dirty_write.insert(range.clone());
                        master_write.insert(range, entry);
                    }
                }
                MemoryMapCommand::Unmap { range, permissions } => {
                    if permissions.read {
                        master_read.remove(range.clone());
                        dirty_read.insert(range.clone());
                    }

                    if permissions.write {
                        master_write.remove(range.clone());
                        dirty_write.insert(range.clone());
                    }
                }
            }
        }

        mirror_dirtying_pass(master_read, &mut dirty_read);
        mirror_dirtying_pass(master_write, &mut dirty_write);

        if !dirty_read.is_empty() {
            let read_table = current_read_page_table.commit(master_read, &dirty_read, runtime);

            let _ = self
                .read_table
                .swap((Some(Owned::new(read_table)), Tag::None), Ordering::AcqRel);
        };

        if !dirty_write.is_empty() {
            let write_table = current_write_page_table.commit(master_write, &dirty_write, runtime);

            let _ = self
                .write_table
                .swap((Some(Owned::new(write_table)), Tag::None), Ordering::AcqRel);
        }
    }
}

#[inline]
fn mirror_dirtying_pass(
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
                let destination_range =
                    RangeInclusive::from_start_and_length(*destination_base, master_region.len());

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
