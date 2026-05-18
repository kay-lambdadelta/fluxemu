use std::ops::RangeInclusive;

use fluxemu_range::RangeIntersection;

use crate::memory::{Address, MemoryMappingTable, PAGE_SIZE, PageEntry};

impl MemoryMappingTable {
    #[inline]
    pub(super) fn overlapping(
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

    /// Grab the single entry (if it exists) that contains this address
    ///
    /// # Safety
    ///
    /// The passed in address must be premasked to fit within the address space
    #[inline]
    pub(super) unsafe fn get(&self, address: Address) -> Option<&PageEntry> {
        let page_index = address / PAGE_SIZE;

        // SAFETY: address divided by PAGE_SIZE should fit into the table provided correct use of this function
        let page = unsafe { self.computed_table.get_unchecked(page_index) };

        // This search implies the entries are sorted and non overlapping (which by the method commit currently uses it is)
        page.iter()
            .take_while(|entry| *entry.range.start() <= address)
            .last()
            .filter(|entry| address <= *entry.range.end())
    }
}
