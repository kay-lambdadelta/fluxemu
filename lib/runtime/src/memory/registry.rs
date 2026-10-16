use std::{
    cell::UnsafeCell,
    collections::HashMap,
    ptr::NonNull,
    range::RangeInclusive,
    sync::{Condvar, Mutex},
};

use bit_vec::BitVec;
use bytes::Bytes;
use fluxemu_math::range::ContiguousRange;
use itertools::Itertools;
use rangemap::RangeInclusiveMap;
use rustc_hash::FxBuildHasher;

use crate::{ResourcePath, RuntimeHandle, memory::CHUNK_SIZE};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct MemoryId(u16);

#[derive(Debug)]
pub(crate) struct RegionInitializationData {
    pub size: usize,
    #[allow(unused)]
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
    storage: Box<[UnsafeCell<u8>]>,
    /// Chunk borrowed tracker
    ///
    /// Note that the last chunk can be smaller than [CHUNK_SIZE]
    borrowed_chunks: Mutex<BitVec<usize>>,
    condvar: Condvar,
}

impl MemoryRegion {
    fn new(size: usize, initial_contents: RangeInclusiveMap<usize, Bytes>) -> Self {
        let mut contents = vec![0u8; size].into_boxed_slice();

        for (addresses, bytes) in initial_contents {
            contents[addresses].copy_from_slice(&bytes);
        }

        // SAFETY: UnsafeCell<u8> is transparent over u8
        let storage = unsafe { Box::from_raw(Box::into_raw(contents) as *mut [UnsafeCell<u8>]) };

        Self {
            borrowed_chunks: Mutex::new(BitVec::from_elem_general(
                size.div_ceil(CHUNK_SIZE),
                false,
            )),
            storage,
            condvar: Condvar::new(),
        }
    }

    fn base_ptr(&self) -> NonNull<[u8]> {
        let base = NonNull::new(UnsafeCell::raw_get(self.storage.as_ptr())).unwrap();

        NonNull::slice_from_raw_parts(base, self.storage.len())
    }

    /// Get the number of chunks required to store this region
    ///
    /// Note that the length of all chunks * [CHUNK_SIZE] is often larger than the actual region sizes
    fn chunk_count(&self) -> usize {
        self.storage.len().div_ceil(CHUNK_SIZE)
    }
}

// SAFETY: Access to the region contents is serialized through `borrowed_chunks`
unsafe impl Sync for MemoryRegion {}

#[derive(Debug)]
pub struct MemoryRegistryData {
    regions: Vec<MemoryRegion>,
    id_for_path: HashMap<ResourcePath, MemoryId, FxBuildHasher>,
}

impl MemoryRegistryData {
    pub fn new(
        required_regions: impl IntoIterator<Item = (ResourcePath, RegionInitializationData)>,
    ) -> Self {
        let mut regions = Vec::new();
        let mut id_for_path = HashMap::default();

        for (
            path,
            RegionInitializationData {
                size,
                sram: _,
                initial_contents,
            },
        ) in required_regions
            .into_iter()
            .sorted_by(|(a, _), (b, _)| a.cmp(b))
        {
            let id = MemoryId(regions.len().try_into().expect("Too many regions"));

            regions.push(MemoryRegion::new(size, initial_contents));

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

    #[inline]
    fn region(&self, id: MemoryId) -> &MemoryRegion {
        &self.regions[id.0 as usize]
    }
}

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
        let region_data = &local_data.region_data_cache[id.0 as usize];

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
            self.acquire_unowned_chunks(id, chunk_range, local_data);
        }

        let region_data = &local_data.region_data_cache[id.0 as usize];

        // SAFETY: The start offset is within a chunk that we own, and within the allocation we made
        let slice_base_ptr = unsafe { region_data.base_ptr.cast::<u8>().as_ptr().add(range.start) };

        // SAFETY: The slice is valid for the region and we own the chunks in the range
        //
        // The range length is additionally guaranteed to be equal to or smaller than the chunks
        //
        // The lifetime of the slice is constrained to the lifetime of the callback
        callback(unsafe { std::slice::from_raw_parts_mut(slice_base_ptr, range.len()) })
    }

    #[cold]
    #[inline]
    fn acquire_unowned_chunks(
        &self,
        id: MemoryId,
        chunk_range: RangeInclusive<usize>,
        local_data: &mut LocalMemoryRegistryData,
    ) {
        let region = self.data().region(id);
        let mut currently_borrowed_chunks = region.borrowed_chunks.lock().unwrap();

        loop {
            let owned_chunks = &mut local_data.region_data_cache[id.0 as usize].owned_chunks;

            let contended = chunk_range.into_iter().any(|chunk_index| {
                !owned_chunks[chunk_index] && currently_borrowed_chunks[chunk_index]
            });

            if !contended {
                for chunk_index in chunk_range {
                    if !owned_chunks[chunk_index] {
                        currently_borrowed_chunks.set(chunk_index, true);
                        owned_chunks.set(chunk_index, true);
                    }
                }

                return;
            }

            drop(currently_borrowed_chunks);
            self.release_all_inner(local_data);
            currently_borrowed_chunks = region.borrowed_chunks.lock().unwrap();

            while chunk_range
                .into_iter()
                .any(|chunk_index| currently_borrowed_chunks[chunk_index])
            {
                currently_borrowed_chunks = region.condvar.wait(currently_borrowed_chunks).unwrap();
            }
        }
    }

    /// # Safety
    ///
    /// No slices handed out by this thread's memory registry may be alive
    pub(crate) unsafe fn release_all(&self) {
        let local_data = unsafe { &mut *self.local_data().get() };

        self.release_all_inner(local_data);
    }

    fn release_all_inner(&self, local_data: &mut LocalMemoryRegistryData) {
        for (id, local_region) in local_data.region_data_cache.iter_mut().enumerate() {
            if local_region.owned_chunks.none() {
                continue;
            }

            let region = &self.data().regions[id];

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

        Some(self.data().region(id).storage.len())
    }
}

pub struct LocalMemoryRegistryData {
    region_data_cache: Vec<LocalMemoryRegionData>,
}

impl LocalMemoryRegistryData {
    pub fn new(data: &MemoryRegistryData) -> Self {
        Self {
            region_data_cache: data
                .regions
                .iter()
                .map(|region| LocalMemoryRegionData {
                    base_ptr: region.base_ptr(),
                    owned_chunks: BitVec::from_elem_general(region.chunk_count(), false),
                })
                .collect(),
        }
    }
}
