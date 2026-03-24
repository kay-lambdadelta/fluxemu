use std::ops::RangeInclusive;

use fluxemu_range::RangeIntersection;

use crate::memory::{Address, MemoryMappingTable, PAGE_SIZE, Page, PageTarget};

pub struct OverlappingMappingsIter<'a> {
    table: &'a MemoryMappingTable,
    cursor_page: &'a Option<Page>,
    access_range: RangeInclusive<Address>,
    page_index: usize,
    end_page: usize,
    entry_index: usize,
}

impl MemoryMappingTable {
    #[inline]
    pub fn overlapping<'a>(
        &'a self,
        access_range: RangeInclusive<Address>,
    ) -> OverlappingMappingsIter<'a> {
        let start_page = access_range.start() / PAGE_SIZE;
        let end_page = access_range.end() / PAGE_SIZE;

        OverlappingMappingsIter {
            cursor_page: &self.computed_table[start_page],
            table: self,
            access_range,
            page_index: start_page,
            end_page,
            entry_index: 0,
        }
    }
}

pub struct Item<'a> {
    pub entry_assigned_range: &'a RangeInclusive<Address>,
    pub target: &'a PageTarget,
}

impl<'a> Iterator for OverlappingMappingsIter<'a> {
    type Item = Item<'a>;

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        while self.page_index <= self.end_page {
            match self.cursor_page {
                Some(Page::Single(entry)) => {
                    if self.entry_index == 0 && self.access_range.intersects(&entry.range) {
                        self.entry_index = 1;

                        return Some(Item {
                            entry_assigned_range: &entry.range,
                            target: &entry.target,
                        });
                    }
                }
                Some(Page::Multi(entries)) => {
                    while self.entry_index < entries.len() {
                        let entry = &entries[self.entry_index];

                        self.entry_index += 1;

                        if self.access_range.intersects(&entry.range) {
                            return Some(Item {
                                entry_assigned_range: &entry.range,
                                target: &entry.target,
                            });
                        }
                    }
                }
                None => {}
            }

            self.page_index += 1;
            self.entry_index = 0;

            if self.page_index <= self.end_page {
                self.cursor_page = &self.table.computed_table[self.page_index];
            }
        }

        None
    }
}
