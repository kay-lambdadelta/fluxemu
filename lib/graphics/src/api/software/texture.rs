use core::ops::{Bound, Deref, DerefMut, Index, IndexMut, RangeBounds, RangeInclusive};

use alloc::boxed::Box;
use bytemuck::{AnyBitPattern, NoUninit};
use nalgebra::{Point2, Vector2};
use rayon::{
    iter::{IndexedParallelIterator, ParallelIterator},
    slice::{ParallelSlice, ParallelSliceMut},
};
use serde::{Deserialize, Serialize};

use fluxemu_range::ContiguousRange;

pub type OwnedTexture<T> = Texture<Box<[T]>>;
pub type RefTexture<'a, T> = Texture<&'a [T]>;
pub type RefMutTexture<'a, T> = Texture<&'a mut [T]>;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Texture<STORAGE: Storage> {
    // The size of texture the storage represents
    storage_size: Vector2<usize>,
    // Offset within the storage's absolute coordinates this view represents
    view_offset: Point2<usize>,
    // The size of the view in pixels, equal to or less than storage_size - view_offset
    view_extent: Vector2<usize>,
    // The storage backing this texture
    storage: STORAGE,
}

impl<STORAGE: Storage> Texture<STORAGE> {
    /// Create a new [`Texture`] with the given dimensions and the default pixel value.
    #[inline]
    pub fn new(width: usize, height: usize) -> Self
    where
        STORAGE: FromIterator<STORAGE::Pixel>,
        STORAGE::Pixel: Default,
    {
        Self::from_fn(width, height, |_| <STORAGE as Storage>::Pixel::default())
    }

    /// Create a new [`Texture`] with the given dimensions and a given pixel value.
    #[inline]
    pub fn from_value(width: usize, height: usize, data: STORAGE::Pixel) -> Self
    where
        STORAGE: FromIterator<STORAGE::Pixel>,
        STORAGE::Pixel: Clone,
    {
        Self::from_fn(width, height, |_| data.clone())
    }

    /// Create a new [`Texture`] with the given dimensions and contents produced by the function
    #[inline]
    pub fn from_fn(
        width: usize,
        height: usize,
        mut producer: impl FnMut(Point2<usize>) -> STORAGE::Pixel,
    ) -> Self
    where
        STORAGE: FromIterator<STORAGE::Pixel>,
    {
        let len = width * height;
        let storage = (0..len)
            .map(|i| producer(Point2::new(i % width, i / width)))
            .collect();

        Self::from_storage(width, height, storage)
    }

    /// Create a new [`Texture`] by wrapping existing storage
    #[inline]
    pub fn from_storage(width: usize, height: usize, storage: STORAGE) -> Self {
        assert_eq!(
            storage.len(),
            width * height,
            "Length of passed in storage does not agree with dimensions"
        );

        Self {
            storage_size: Vector2::new(width, height),
            view_offset: Point2::new(0, 0),
            view_extent: Vector2::new(width, height),
            storage,
        }
    }

    /// Index the texture without bounds checking
    ///
    /// # Safety
    ///
    /// Access must not be out of bounds
    #[inline]
    pub unsafe fn get_unchecked(&self, point: impl Into<Point2<usize>>) -> &STORAGE::Pixel {
        let point = point.into();

        debug_assert!(point.x < self.width());
        debug_assert!(point.y < self.height());

        let index =
            (self.view_offset.y + point.y) * self.storage_size.x + (self.view_offset.x + point.x);

        debug_assert!(index < self.storage.len());

        unsafe { self.storage.get_unchecked(index) }
    }

    /// Index the texture mutably without bounds checking
    ///
    /// # Safety
    ///
    /// Access must not be out of bounds
    #[inline]
    pub unsafe fn get_unchecked_mut(
        &mut self,
        point: impl Into<Point2<usize>>,
    ) -> &mut STORAGE::Pixel
    where
        STORAGE: StorageMut,
    {
        let point = point.into();

        debug_assert!(point.x < self.width());
        debug_assert!(point.y < self.height());

        let index =
            (self.view_offset.y + point.y) * self.storage_size.x + (self.view_offset.x + point.x);

        debug_assert!(index < self.storage.len());

        unsafe { self.storage.get_unchecked_mut(index) }
    }

    /// Get the width of the texture (or the view, if applicable)
    #[inline]
    pub fn width(&self) -> usize {
        self.view_extent.x
    }

    /// Get the height of the texture (or the view, if applicable)
    #[inline]
    pub fn height(&self) -> usize {
        self.view_extent.y
    }

    /// Get the size of the texture (or the view, if applicable)
    #[inline]
    pub fn size(&self) -> Vector2<usize> {
        Vector2::new(self.width(), self.height())
    }

    /// Produce a same sized or smaller view borrowing from this textures storage
    #[inline]
    pub fn view(
        &self,
        x: impl RangeBounds<usize>,
        y: impl RangeBounds<usize>,
    ) -> RefTexture<'_, STORAGE::Pixel> {
        let (x0, x1) = resolve_range(x, self.width());
        let (y0, y1) = resolve_range(y, self.height());

        let start = Point2::new(x0, y0);
        let end = Point2::new(x1, y1);

        Texture {
            storage: &self.storage,
            view_extent: end - start,
            view_offset: start,
            storage_size: self.storage_size,
        }
    }

    /// Produce a same sized or smaller view mutably borrowing from this textures storage
    #[inline]
    pub fn view_mut(
        &mut self,
        x: impl RangeBounds<usize>,
        y: impl RangeBounds<usize>,
    ) -> RefMutTexture<'_, STORAGE::Pixel>
    where
        STORAGE: StorageMut,
    {
        let (x0, x1) = resolve_range(x, self.width());
        let (y0, y1) = resolve_range(y, self.height());

        let start = Point2::new(x0, y0);
        let end = Point2::new(x1, y1);

        Texture {
            storage: &mut self.storage,
            view_extent: end - start,
            view_offset: start,
            storage_size: self.storage_size,
        }
    }

    /// Produce a same sized or smaller view taking this texture's storage
    #[inline]
    pub fn view_owned(self, x: impl RangeBounds<usize>, y: impl RangeBounds<usize>) -> Self {
        let (x0, x1) = resolve_range(x, self.width());
        let (y0, y1) = resolve_range(y, self.height());

        let start = Point2::new(x0, y0);
        let end = Point2::new(x1, y1);

        Texture {
            storage: self.storage,
            view_extent: end - start,
            view_offset: start,
            storage_size: self.storage_size,
        }
    }

    /// Copy a region from another texture into this texture
    #[inline]
    pub fn copy_from<T2: Into<STORAGE::Pixel> + Clone + 'static>(
        &mut self,
        other: impl AsViewTexture<T2>,
        mode: CopyMode,
    ) where
        STORAGE: StorageMut,
    {
        let other = other.as_view();

        if self.size() == other.size() {
            if self.storage_size == other.storage_size {
                for (index, pixel) in other.storage.iter().enumerate() {
                    self.storage[index] = pixel.clone().into();
                }
            } else {
                for y in 0..self.height() {
                    for x in 0..self.width() {
                        let index = Point2::new(x, y);

                        self[index] = other[index].clone().into();
                    }
                }
            }

            return;
        }

        match mode {
            CopyMode::Nearest => {
                let ratio = Vector2::new(
                    other.width() as f32 / self.width() as f32,
                    other.height() as f32 / self.height() as f32,
                );

                for y in 0..self.height() {
                    for x in 0..self.width() {
                        let source_position = Point2::new(
                            (x as f32 * ratio.x) as usize,
                            (y as f32 * ratio.y) as usize,
                        );

                        self[Point2::new(x, y)] = other[source_position].clone().into();
                    }
                }
            }
        }
    }

    /// Flip this texture on its x-axis
    #[inline]
    pub fn flip_x(&mut self)
    where
        STORAGE: StorageMut,
        STORAGE::Pixel: Clone + 'static,
    {
        let width = self.width();
        let height = self.height();

        for y in 0..height {
            for x in 0..width / 2 {
                let first_coord = Point2::new(x, y);
                let second_coord = Point2::new(width - x - 1, y);

                let a = self[first_coord].clone();
                let b = self[second_coord].clone();

                self[first_coord] = b;
                self[second_coord] = a;
            }
        }
    }

    /// Flip this texture on its y-axis
    #[inline]
    pub fn flip_y(&mut self)
    where
        STORAGE: StorageMut,
        STORAGE::Pixel: Clone + 'static,
    {
        let width = self.width();
        let height = self.height();

        for y in 0..height / 2 {
            for x in 0..width {
                let first_coord = Point2::new(x, y);
                let second_coord = Point2::new(x, height - y - 1);

                let a = self[first_coord].clone();
                let b = self[second_coord].clone();

                self[first_coord] = b;
                self[second_coord] = a;
            }
        }
    }

    /// Fill the entire texture with a single value
    #[inline]
    pub fn fill(&mut self, value: STORAGE::Pixel)
    where
        STORAGE: StorageMut,
        STORAGE::Pixel: Clone,
    {
        // Each branch is a less efficient way of filling the texture

        if self.view_offset == Point2::new(0, 0) && self.storage_size == self.view_extent {
            // The view on the storage is wholly overlapping the storage
            self.storage.fill(value);
        } else if self.view_extent.x == self.storage_size.x {
            // This is a single contiguous memory block
            let range = RangeInclusive::from_start_and_length(
                self.view_offset.y * self.storage_size.x,
                self.view_extent.x * self.view_extent.y,
            );

            self.storage[range].fill(value)
        } else {
            // Fall back onto filling the texture row by row.
            for row in self.rows_mut() {
                row.fill(value.clone());
            }
        }
    }

    #[inline]
    pub fn rows(&self) -> impl Iterator<Item = &[STORAGE::Pixel]> {
        let view_extent = self.view_extent;
        let view_offset = self.view_offset;

        self.storage
            .chunks_exact(self.storage_size.x)
            .skip(view_offset.y)
            .take(view_extent.y)
            .map(move |row| &row[view_offset.x..view_offset.x + view_extent.x])
    }

    #[inline]
    pub fn rows_mut(&mut self) -> impl Iterator<Item = &mut [STORAGE::Pixel]>
    where
        STORAGE: StorageMut,
    {
        let view_extent = self.view_extent;
        let view_offset = self.view_offset;

        self.storage
            .chunks_exact_mut(self.storage_size.x)
            .skip(view_offset.y)
            .take(view_extent.y)
            .map(move |row| &mut row[view_offset.x..view_offset.x + view_extent.x])
    }

    /// Iterate over the rows of this texture in parallel
    #[inline]
    pub fn par_rows(&self) -> impl IndexedParallelIterator<Item = &[STORAGE::Pixel]>
    where
        STORAGE::Pixel: Send + Sync,
    {
        let view_extent = self.view_extent;
        let view_offset = self.view_offset;

        self.storage
            .par_chunks_exact(self.storage_size.x)
            .skip(view_offset.y)
            .take(view_extent.y)
            .map(move |row| &row[view_offset.x..view_offset.x + view_extent.x])
    }

    /// Mutably iterate over the rows of this texture in parallel
    #[inline]
    pub fn par_rows_mut(&mut self) -> impl IndexedParallelIterator<Item = &mut [STORAGE::Pixel]>
    where
        STORAGE: StorageMut,
        STORAGE::Pixel: Send + Sync,
    {
        let view_extent = self.view_extent;
        let view_offset = self.view_offset;

        self.storage
            .par_chunks_exact_mut(self.storage_size.x)
            .skip(view_offset.y)
            .take(view_extent.y)
            .map(move |row| &mut row[view_offset.x..view_offset.x + view_extent.x])
    }

    /// Iterate over the pixels in this texture
    #[inline]
    pub fn iter_pixels(&self) -> impl Iterator<Item = &STORAGE::Pixel> {
        self.rows().flatten()
    }

    /// Iterate over the pixels in this texture with their x,y coordinates
    #[inline]
    pub fn iter_pixels_indexed(&self) -> impl Iterator<Item = (Point2<usize>, &STORAGE::Pixel)> {
        self.rows().enumerate().flat_map(move |(y, row)| {
            row.iter()
                .enumerate()
                .map(move |(x, pixel)| (Point2::new(x, y), pixel))
        })
    }

    /// Mutably iterate over the pixels in this texture
    #[inline]
    pub fn iter_pixels_mut(&mut self) -> impl Iterator<Item = &mut STORAGE::Pixel>
    where
        STORAGE: StorageMut,
    {
        self.rows_mut().flatten()
    }

    /// Mutably iterate over the pixels in this texture with their x,y coordinates
    #[inline]
    pub fn iter_pixels_indexed_mut(
        &mut self,
    ) -> impl Iterator<Item = (Point2<usize>, &mut STORAGE::Pixel)>
    where
        STORAGE: StorageMut,
    {
        self.rows_mut().enumerate().flat_map(move |(y, row)| {
            row.iter_mut()
                .enumerate()
                .map(move |(x, pixel)| (Point2::new(x, y), pixel))
        })
    }

    /// Try to produce a slice of the storage backing this texture.
    ///
    /// Returns `None` if the texture is not contiguous in memory.
    #[inline]
    pub fn as_slice(&self) -> Option<&[STORAGE::Pixel]> {
        if self.storage_size.x == self.view_extent.x {
            let slice = &self.storage[RangeInclusive::from_start_and_length(
                self.view_offset.y * self.storage_size.x,
                self.view_extent.y * self.view_extent.x,
            )];

            Some(slice)
        } else {
            None
        }
    }

    /// Try to produce a mutable slice of the storage backing this texture.
    ///
    /// Returns `None` if the texture is not contiguous in memory.
    #[inline]
    pub fn as_slice_mut(&mut self) -> Option<&mut [STORAGE::Pixel]>
    where
        STORAGE: StorageMut,
    {
        if self.storage_size.x == self.view_extent.x {
            let slice = &mut self.storage[RangeInclusive::from_start_and_length(
                self.view_offset.y * self.storage_size.x,
                self.view_extent.y * self.view_extent.x,
            )];

            Some(slice)
        } else {
            None
        }
    }

    #[inline]
    pub fn storage(&self) -> &STORAGE {
        &self.storage
    }

    #[inline]
    pub fn storage_mut(&mut self) -> &mut STORAGE
    where
        STORAGE: StorageMut,
    {
        &mut self.storage
    }

    /// Form a view of this texture with a different pixel type of compatible layout
    #[inline]
    pub fn cast<T2: NoUninit + AnyBitPattern>(&self) -> Texture<&[T2]>
    where
        STORAGE::Pixel: NoUninit + AnyBitPattern,
    {
        assert_eq!(size_of::<STORAGE::Pixel>(), size_of::<T2>());

        Texture {
            storage_size: self.storage_size,
            view_offset: self.view_offset,
            view_extent: self.view_extent,
            storage: bytemuck::cast_slice(&self.storage),
        }
    }

    /// From a mutable view of this texture with a different pixel type of compatible layout
    #[inline]
    pub fn cast_mut<T2: NoUninit + AnyBitPattern>(&mut self) -> Texture<&mut [T2]>
    where
        STORAGE: StorageMut,
        STORAGE::Pixel: NoUninit + AnyBitPattern,
    {
        assert_eq!(size_of::<STORAGE::Pixel>(), size_of::<T2>());

        Texture {
            storage_size: self.storage_size,
            view_offset: self.view_offset,
            view_extent: self.view_extent,
            storage: bytemuck::cast_slice_mut(&mut self.storage),
        }
    }
}

impl<STORAGE: Storage, P: Into<Point2<usize>>> Index<P> for Texture<STORAGE> {
    type Output = STORAGE::Pixel;

    #[inline]
    fn index(&self, point: P) -> &Self::Output {
        let point = point.into();

        assert!(point.x < self.width());
        assert!(point.y < self.height());

        unsafe { self.get_unchecked(point) }
    }
}

impl<STORAGE: StorageMut, P: Into<Point2<usize>>> IndexMut<P> for Texture<STORAGE> {
    #[inline]
    fn index_mut(&mut self, point: P) -> &mut Self::Output {
        let point = point.into();

        assert!(point.x < self.width());
        assert!(point.y < self.height());

        unsafe { self.get_unchecked_mut(point) }
    }
}

#[inline]
fn resolve_range(range: impl RangeBounds<usize>, max: usize) -> (usize, usize) {
    let start = match range.start_bound() {
        Bound::Included(&v) => v,
        Bound::Excluded(&v) => v + 1,
        Bound::Unbounded => 0,
    };

    let end = match range.end_bound() {
        Bound::Included(&v) => v + 1,
        Bound::Excluded(&v) => v,
        Bound::Unbounded => max,
    };

    assert!(start <= end);
    assert!(end <= max);

    (start, end)
}

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum CopyMode {
    Nearest,
}

pub trait AsViewTexture<P> {
    fn as_view(&self) -> RefTexture<'_, P>;
}
impl<P, STORAGE: Storage<Pixel = P>> AsViewTexture<P> for Texture<STORAGE> {
    #[inline]
    fn as_view(&self) -> RefTexture<'_, P> {
        self.view(.., ..)
    }
}
impl<P, STORAGE: Storage<Pixel = P>> AsViewTexture<P> for &Texture<STORAGE> {
    #[inline]
    fn as_view(&self) -> RefTexture<'_, P> {
        self.view(.., ..)
    }
}
impl<P, STORAGE: Storage<Pixel = P>> AsViewTexture<P> for &mut Texture<STORAGE> {
    #[inline]
    fn as_view(&self) -> RefTexture<'_, P> {
        self.view(.., ..)
    }
}

pub trait AsViewTextureMut<P>: AsViewTexture<P> {
    fn as_view_mut(&mut self) -> RefMutTexture<'_, P>;
}
impl<P, STORAGE: StorageMut<Pixel = P>> AsViewTextureMut<P> for Texture<STORAGE> {
    #[inline]
    fn as_view_mut(&mut self) -> RefMutTexture<'_, P> {
        self.view_mut(.., ..)
    }
}
impl<P, STORAGE: StorageMut<Pixel = P>> AsViewTextureMut<P> for &mut Texture<STORAGE> {
    #[inline]
    fn as_view_mut(&mut self) -> RefMutTexture<'_, P> {
        self.view_mut(.., ..)
    }
}

pub trait Storage: Deref<Target = [Self::Pixel]> {
    type Pixel;
}
impl<T, STORAGE: Deref<Target = [T]>> Storage for STORAGE {
    type Pixel = T;
}

pub trait StorageMut: Storage + DerefMut {}
impl<T, STORAGE: DerefMut<Target = [T]>> StorageMut for STORAGE {}
