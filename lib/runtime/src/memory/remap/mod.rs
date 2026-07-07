use std::{ops::RangeInclusive, sync::atomic::Ordering};

use fluxemu_range::ContiguousRange;
use rangemap::RangeInclusiveSet;
use sdd::{Guard, Owned, Tag};

use crate::{
    RuntimeHandle,
    memory::{
        AddressSpaceData, MapTarget, MasterTableEntry, MasterTables, Members, MemoryMapCommand,
        PageTable,
    },
    scheduler::Period,
};

mod commit;

impl AddressSpaceData {
    #[inline]
    pub fn remap(
        &self,
        _timestamp: &Period,
        guard: &Guard,
        runtime: &RuntimeHandle,
        commands: impl IntoIterator<Item = MemoryMapCommand>,
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
                        MapTarget::ImmutableMemory(bytes) => {
                            if permissions.read {
                                master_read.insert(
                                    range.clone(),
                                    MasterTableEntry::ImmutableMemory(bytes.clone()),
                                );
                            }

                            if permissions.write {
                                unreachable!("Buffers are read only");
                            }
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
                        MapTarget::Memory { path, subrange } => {
                            let region_size = runtime.memory_registry().region_size(&path).unwrap();

                            let subrange = subrange.clone().unwrap_or_else(|| {
                                RangeInclusive::from_start_and_length(0, region_size)
                            });

                            assert!(range.len() <= subrange.len());

                            if permissions.read {
                                master_read.insert(
                                    range.clone(),
                                    MasterTableEntry::Memory {
                                        path: path.clone(),
                                        source_base: *range.start(),
                                        region_base: *subrange.start(),
                                        length: range.len(),
                                    },
                                );
                            }

                            if permissions.write {
                                master_write.insert(
                                    range.clone(),
                                    MasterTableEntry::Memory {
                                        path: path.clone(),
                                        source_base: *range.start(),
                                        region_base: *subrange.start(),
                                        length: range.len(),
                                    },
                                );
                            }
                        }
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

        read_table.mirror_dirtying_pass(master_read, &mut dirty_read);
        write_table.mirror_dirtying_pass(master_write, &mut dirty_write);

        read_table.commit(
            &current_members.read,
            master_read,
            &dirty_read,
            runtime.component_registry(),
            runtime.memory_registry(),
        );
        write_table.commit(
            &current_members.write,
            master_write,
            &dirty_write,
            runtime.component_registry(),
            runtime.memory_registry(),
        );

        let members = Members {
            read: read_table,
            write: write_table,
        };

        let _ = self
            .members
            .swap((Some(Owned::new(members)), Tag::None), Ordering::AcqRel);
    }
}
