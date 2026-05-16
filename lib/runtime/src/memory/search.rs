use std::ops::RangeInclusive;

use fluxemu_range::{RangeBase, RangeIntersection};

use crate::memory::{Address, MemoryMappingTable, PAGE_SIZE, PageEntry};

impl MemoryMappingTable {
    #[inline]
    pub fn overlapping(
        &self,
        access_range: RangeInclusive<Address>,
    ) -> impl Iterator<Item = &PageEntry> {
        let start_page = access_range.start() / PAGE_SIZE;
        let end_page = access_range.end() / PAGE_SIZE;

        self.computed_table[start_page..=end_page]
            .iter()
            .flatten()
            .filter(move |page_entry| access_range.intersects(&page_entry.range))
    }

    #[inline]
    pub fn get(&self, address: Address) -> Option<&PageEntry> {
        let page_index = address / PAGE_SIZE;
        let page = &self.computed_table[page_index];

        page.iter()
            .find(|entry| RangeBase::contains(&entry.range, &address))
    }
}
