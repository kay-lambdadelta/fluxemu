use std::ops::RangeInclusive;

use fluxemu_range::RangeIntersection;

use crate::memory::{Address, MemoryMappingTable, PAGE_SIZE, Page, PageEntry};

pub(super) struct OverlappingMappingsIter<'a> {
    pages: &'a [Option<Page>],
    access_range: RangeInclusive<Address>,
    entry_index: usize,
}

impl MemoryMappingTable {
    #[inline]
    pub fn overlapping(
        &self,
        access_range: RangeInclusive<Address>,
    ) -> OverlappingMappingsIter<'_> {
        let start_page = access_range.start() / PAGE_SIZE;
        let end_page = access_range.end() / PAGE_SIZE;

        OverlappingMappingsIter {
            pages: &self.computed_table[start_page..=end_page],
            access_range,
            entry_index: 0,
        }
    }

    #[inline]
    pub fn get(&self, address: Address) -> Option<&PageEntry> {
        let page = address / PAGE_SIZE;

        match self.computed_table[page].as_ref()? {
            Page::Single(entry) => {
                if entry.range.contains(&address) {
                    return Some(entry);
                }
            }
            Page::Multi(entries) => {
                for entry in entries {
                    if entry.range.contains(&address) {
                        return Some(entry);
                    }
                }
            }
        }

        None
    }
}

impl<'a> Iterator for OverlappingMappingsIter<'a> {
    type Item = &'a PageEntry;

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        loop {
            let (current, rest) = self.pages.split_first()?;

            match current {
                Some(Page::Single(entry)) => {
                    self.pages = rest;
                    // Not used for single entry pages
                    self.entry_index = 0;

                    if self.access_range.intersects(&entry.range) {
                        return Some(entry);
                    }
                }
                Some(Page::Multi(entries)) => {
                    while self.entry_index < entries.len() {
                        let entry = &entries[self.entry_index];

                        self.entry_index += 1;

                        if self.access_range.intersects(&entry.range) {
                            return Some(entry);
                        }
                    }

                    self.pages = rest;
                    self.entry_index = 0;
                }
                None => {
                    self.pages = rest;
                    self.entry_index = 0;
                }
            }
        }
    }
}
