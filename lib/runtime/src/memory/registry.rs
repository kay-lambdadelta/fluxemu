use std::{
    alloc::Layout,
    cell::UnsafeCell,
    collections::HashMap,
    ptr::NonNull,
    range::RangeInclusive,
    sync::{Condvar, Mutex},
};

use bit_vec::BitVec;
use bytes::Bytes;
use fluxemu_range::ContiguousRange;
use itertools::Itertools;
use rangemap::RangeInclusiveMap;
use rustc_hash::FxBuildHasher;

use crate::{ResourcePath, RuntimeHandle, memory::CHUNK_SIZE};

pub type MemoryId = u16;

#[derive(Debug)]
pub(crate) struct RegionInitializationData {
    pub size: usize,
    pub sram: bool,
    pub initial_contents: RangeInclusiveMap<usize, Bytes>,
}

#[derive(Debug)]
struct LocalMemoryRegionData {
    /// Pointer to the allocation
    base_ptr: NonNull<[u8]>,
    /// Chunks this thread currently owns and implicitly how large this region is in chunks
    owned_chunks: BitVec<usize>,
}

#[derive(Debug)]
struct MemoryRegion {
    /// Pointer to the allocation
    base_ptr: NonNull<[u8]>,
    /// Chunk borrowed tracker
    ///
    /// Note that the last chunk can be smaller than [CHUNK_SIZE]
    borrowed_chunks: Mutex<BitVec<usize>>,
    condvar: Condvar,
}

impl MemoryRegion {
    /// Get the number of chunks required to store this region
    ///
    /// Note that the length of all chunks * [CHUNK_SIZE] is often larger than the actual region sizes
    fn chunk_count(&self) -> usize {
        self.base_ptr.len().div_ceil(CHUNK_SIZE)
    }
}

impl Drop for MemoryRegion {
    fn drop(&mut self) {
        unsafe {
            std::alloc::dealloc(
                self.base_ptr.cast().as_ptr(),
                Layout::array::<u8>(self.base_ptr.len()).unwrap(),
            );
        }
    }
}

#[derive(Debug)]
pub struct MemoryRegistryData {
    regions: HashMap<MemoryId, MemoryRegion, FxBuildHasher>,
    id_for_path: HashMap<ResourcePath, MemoryId, FxBuildHasher>,
}

impl MemoryRegistryData {
    pub fn new(required_regions: HashMap<ResourcePath, RegionInitializationData>) -> Self {
        let mut regions = HashMap::default();
        let mut id_for_path = HashMap::default();
        let mut next_id: MemoryId = 0;

        for (
            path,
            RegionInitializationData {
                size,
                sram: _,
                initial_contents,
            },
        ) in required_regions
        {
            let id = next_id;
            next_id = next_id.checked_add(1).expect("Too many regions");

            let region_chunk_count = size.div_ceil(CHUNK_SIZE);
            let layout = Layout::array::<u8>(size).expect("Invalid memory region layout");

            let allocation = unsafe { std::alloc::alloc_zeroed(layout) };
            let mut base_ptr = NonNull::slice_from_raw_parts(
                NonNull::new(allocation).unwrap_or_else(|| std::alloc::handle_alloc_error(layout)),
                size,
            );

            if !initial_contents.is_empty() {
                // SAFETY: We validated that this pointer is a valid allocation
                let representation_slice = unsafe { base_ptr.as_mut() };

                for (addresses, bytes) in initial_contents {
                    representation_slice[addresses].copy_from_slice(&bytes);
                }
            }

            regions.insert(
                id,
                MemoryRegion {
                    base_ptr,
                    borrowed_chunks: Mutex::new(BitVec::from_elem_general(
                        region_chunk_count,
                        false,
                    )),
                    condvar: Condvar::new(),
                },
            );

            assert!(
                id_for_path.insert(path.clone(), id).is_none(),
                "Duplicate memory region path: {path}",
            );
        }

        Self {
            regions,
            id_for_path,
        }
    }

    pub fn id_for_path(&self, path: &ResourcePath) -> Option<MemoryId> {
        self.id_for_path.get(path).copied()
    }
}

// SAFETY: We manage the raw pointers to memory ourselves
unsafe impl Send for MemoryRegistryData {}
unsafe impl Sync for MemoryRegistryData {}

#[derive(Debug, Clone, Copy)]
pub struct MemoryRegistry<'a> {
    runtime: &'a RuntimeHandle,
}

impl<'a> MemoryRegistry<'a> {
    pub fn new(runtime: &'a RuntimeHandle) -> Self {
        Self { runtime }
    }

    #[inline]
    fn data(&self) -> &MemoryRegistryData {
        &self.runtime.machine().memory_registry_data
    }

    #[inline]
    fn local_data(&self) -> &UnsafeCell<LocalMemoryRegistryData> {
        &self.runtime.local_data().memory_registry_data
    }

    #[inline(always)]
    fn get_memory_of_region(
        &self,
        id: MemoryId,
        range: RangeInclusive<usize>,
        callback: impl FnOnce(&mut [u8]),
    ) {
        // Ensure the range is actually valid
        assert!(!range.is_empty());

        let local_data = unsafe { &mut *self.local_data().get() };
        let region_data = local_data.region_data_cache.get_mut(id as usize).unwrap();

        // This check is sufficient because we assert earlier that the range is a valid range in which "last" is larger or equal to "start"
        assert!(range.last < region_data.base_ptr.len());

        let chunk_range = RangeInclusive {
            start: range.start / CHUNK_SIZE,
            last: range.last / CHUNK_SIZE,
        };

        let needs_acquisition = !chunk_range.into_iter().all(|index| {
            // SAFETY: The chunk range is valid for the region due to the previous checks
            unsafe { region_data.owned_chunks.get_unchecked(index) }
        });

        if needs_acquisition {
            self.acquire_unowned_chunks(id, chunk_range, &mut region_data.owned_chunks);
        }

        // SAFETY: The start offset is within a chunk that we own, and within the allocation we made
        let slice_base_ptr = unsafe { region_data.base_ptr.cast::<u8>().as_ptr().add(range.start) };

        // SAFETY: The slice is valid for the region and we own the chunks in the range
        //
        // The range length is additionally guaranteed to be equal to or smaller than the chunks
        //
        // The lifetime of the slice is constrained to the lifetime of the callback
        callback(unsafe { std::slice::from_raw_parts_mut(slice_base_ptr.cast(), range.len()) })
    }

    #[cold]
    #[inline]
    fn acquire_unowned_chunks(
        &self,
        id: MemoryId,
        chunk_range: RangeInclusive<usize>,
        owned_chunks: &mut BitVec<usize>,
    ) {
        let region = self.data().regions.get(&id).unwrap();

        let mut currently_borrowed_chunks = region.borrowed_chunks.lock().unwrap();
        let mut claimed_this_attempt = Vec::new();

        'attempt: loop {
            claimed_this_attempt.clear();

            for chunk_index in chunk_range {
                if owned_chunks[chunk_index] {
                    continue;
                }

                if currently_borrowed_chunks[chunk_index] {
                    for &claimed in &claimed_this_attempt {
                        currently_borrowed_chunks.set(claimed, false);
                    }
                    region.condvar.notify_all();

                    currently_borrowed_chunks =
                        region.condvar.wait(currently_borrowed_chunks).unwrap();

                    continue 'attempt;
                }

                currently_borrowed_chunks.set(chunk_index, true);
                claimed_this_attempt.push(chunk_index);
            }

            for &chunk_index in &claimed_this_attempt {
                owned_chunks.set(chunk_index, true);
            }

            break;
        }
    }

    pub fn release_all(&self) {
        let local_data = unsafe { &mut *self.local_data().get() };

        for (id, local_region) in local_data.region_data_cache.iter_mut().enumerate() {
            let region = self.data().regions.get(&(id as MemoryId)).unwrap();

            let mut currently_borrowed_chunks = region.borrowed_chunks.lock().unwrap();

            for (chunk_index, mut owned) in local_region.owned_chunks.iter_mut().enumerate() {
                if *owned {
                    assert!(
                        currently_borrowed_chunks[chunk_index],
                        "Desync: chunk {chunk_index} from memory region {id} is marked as locally \
                         owned but not globally borrowed"
                    );

                    currently_borrowed_chunks.set(chunk_index, false);
                    *owned = false;
                }
            }

            drop(currently_borrowed_chunks);
            region.condvar.notify_all();
        }
    }

    #[inline]
    pub fn read(&self, id: MemoryId, offset: usize, buffer: &mut [u8]) {
        let range = RangeInclusive::from_start_and_length(offset, buffer.len());

        self.get_memory_of_region(
            id,
            range,
            #[inline]
            |relevant_region_memory| {
                buffer.copy_from_slice(relevant_region_memory);
            },
        );
    }

    #[inline]
    pub fn write(&self, id: MemoryId, offset: usize, buffer: &[u8]) {
        let range = RangeInclusive::from_start_and_length(offset, buffer.len());

        self.get_memory_of_region(
            id,
            range,
            #[inline]
            |relevant_region_memory| {
                relevant_region_memory.copy_from_slice(buffer);
            },
        );
    }

    #[inline]
    pub fn id_for_path(&self, path: &ResourcePath) -> Option<MemoryId> {
        self.data().id_for_path(path)
    }

    #[inline]
    pub fn region_size(&self, path: &ResourcePath) -> Option<usize> {
        let id = self.id_for_path(path)?;

        Some(self.data().regions[&id].base_ptr.len())
    }
}

pub struct LocalMemoryRegistryData {
    region_data_cache: Vec<LocalMemoryRegionData>,
}

impl LocalMemoryRegistryData {
    pub fn new(data: &MemoryRegistryData) -> Self {
        Self {
            region_data_cache: Vec::from_iter(
                data.regions
                    .iter()
                    .sorted_by_key(|(id, _)| *id)
                    .map(|(_, region)| LocalMemoryRegionData {
                        base_ptr: region.base_ptr,
                        owned_chunks: BitVec::from_elem_general(region.chunk_count(), false),
                    }),
            ),
        }
    }
}
