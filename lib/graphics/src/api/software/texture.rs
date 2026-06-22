use core::ops::{Bound, Deref, DerefMut, Index, IndexMut, RangeBounds, RangeInclusive};

use alloc::vec::Vec;
use bytemuck::{AnyBitPattern, NoUninit};
use nalgebra::{Point2, Vector2};
use rayon::{
    iter::{IndexedParallelIterator, ParallelIterator},
    slice::{ParallelSlice, ParallelSliceMut},
};
use serde::{Deserialize, Serialize};

use fluxemu_range::ContiguousRange;

pub type OwnedTexture<T> = Texture<Vec<T>>;
pub type RefTexture<'a, T> = Texture<&'a [T]>;
pub type RefMutTexture<'a, T> = Texture<&'a mut [T]>;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Texture<STORAGE: Storage> {
    texture_size: Vector2<usize>,
    size: Vector2<usize>,
    offset: Point2<usize>,
    storage: STORAGE,
}

impl<STORAGE: Storage> Texture<STORAGE> {
    #[inline]
    pub fn new(width: usize, height: usize, data: STORAGE::Pixel) -> Self
    where
        STORAGE: FromIterator<STORAGE::Pixel>,
        STORAGE::Pixel: Clone,
    {
        let len = width * height;

        Self {
            storage: (0..len).map(|_| data.clone()).collect(),
            size: Vector2::new(width, height),
            texture_size: Vector2::new(width, height),
            offset: Point2::new(0, 0),
        }
    }

    #[inline]
    pub fn from_fn(
        width: usize,
        height: usize,
        mut producer: impl FnMut(usize, usize) -> STORAGE::Pixel,
    ) -> Self
    where
        STORAGE: FromIterator<STORAGE::Pixel>,
    {
        let len = width * height;

        Self {
            storage: (0..len).map(|i| producer(i % width, i / width)).collect(),
            size: Vector2::new(width, height),
            texture_size: Vector2::new(width, height),
            offset: Point2::new(0, 0),
        }
    }

    #[inline]
    pub fn from_storage(width: usize, height: usize, storage: STORAGE) -> Self {
        let size = Vector2::new(width, height);
        assert_eq!(storage.len(), size.product());

        Self {
            storage,
            size,
            texture_size: Vector2::new(width, height),
            offset: Point2::new(0, 0),
        }
    }

    #[inline]
    pub fn from_storage_with_stride(
        width: usize,
        height: usize,
        stride: usize,
        storage: STORAGE,
    ) -> Self {
        assert!(stride >= width, "stride must be >= width");
        assert!(
            storage.len() >= stride * height,
            "slice is too small ({}) for the given height ({}) and stride ({})",
            storage.len(),
            height,
            stride
        );

        Self {
            storage,
            texture_size: Vector2::new(stride, height),
            offset: Point2::new(0, 0),
            size: Vector2::new(width, height),
        }
    }

    ///  # Safety
    ///     Access must not be out of bounds
    #[inline]
    pub unsafe fn get_unchecked(&self, point: impl Into<Point2<usize>>) -> &STORAGE::Pixel {
        let point = point.into();

        debug_assert!(point.x < self.width());
        debug_assert!(point.y < self.height());

        let index = (self.offset.y + point.y) * self.texture_size.x + (self.offset.x + point.x);

        unsafe { self.storage.get_unchecked(index) }
    }

    ///  # Safety
    ///     Access must not be out of bounds
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

        let index = (self.offset.y + point.y) * self.texture_size.x + (self.offset.x + point.x);

        unsafe { self.storage.get_unchecked_mut(index) }
    }

    #[inline]
    pub fn width(&self) -> usize {
        self.size.x
    }

    #[inline]
    pub fn height(&self) -> usize {
        self.size.y
    }

    #[inline]
    pub fn stride(&self) -> usize {
        self.texture_size.x
    }

    #[inline]
    pub fn size(&self) -> Vector2<usize> {
        Vector2::new(self.width(), self.height())
    }

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
            storage: &self.storage[..],
            texture_size: self.size,
            offset: start,
            size: end - start,
        }
    }

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
            storage: &mut self.storage[..],
            texture_size: self.size,
            offset: start,
            size: end - start,
        }
    }

    #[inline]
    pub fn copy_from<T2: Into<STORAGE::Pixel> + Clone + 'static>(
        &mut self,
        other: &Texture<impl Storage<Pixel = T2>>,
        mode: CopyMode,
    ) where
        STORAGE: StorageMut,
    {
        if self.size() == other.size() {
            for y in 0..self.height() {
                for x in 0..self.width() {
                    let index = Point2::new(x, y);

                    self[index] = other[index].clone().into();
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

    #[inline]
    pub fn fill(&mut self, value: STORAGE::Pixel)
    where
        STORAGE: StorageMut,
        STORAGE::Pixel: Clone,
    {
        if self.offset.x == 0 && self.size.x == self.texture_size.x {
            let range = RangeInclusive::from_start_and_length(
                self.offset.y * self.texture_size.x,
                self.size.y * self.texture_size.x,
            );

            self.storage[range].fill(value);
        } else {
            let x_range = RangeInclusive::from_start_and_length(self.offset.x, self.size.x);

            for row in self
                .storage
                .chunks_exact_mut(self.texture_size.x)
                .skip(self.offset.y)
                .take(self.size.y)
            {
                row[x_range.clone()].fill(value.clone());
            }
        }
    }

    #[inline]
    pub fn iter_pixels(&self) -> impl Iterator<Item = &STORAGE::Pixel> {
        let view_width = self.width();
        let offset = self.offset;
        let texture_width = self.texture_size.x;

        self.storage
            .chunks_exact(texture_width)
            .skip(offset.y)
            .take(self.size.y)
            .flat_map(move |row| row[offset.x..offset.x + view_width].iter())
    }

    #[inline]
    pub fn iter_pixels_indexed(&self) -> impl Iterator<Item = (Point2<usize>, &STORAGE::Pixel)> {
        let view_width = self.width();
        let offset = self.offset;
        let texture_width = self.texture_size.x;

        self.storage
            .chunks_exact(texture_width)
            .skip(offset.y)
            .take(self.size.y)
            .enumerate()
            .flat_map(move |(y, row)| {
                row[offset.x..offset.x + view_width]
                    .iter()
                    .enumerate()
                    .map(move |(x, pixel)| (Point2::new(x, y), pixel))
            })
    }

    #[inline]
    pub fn iter_pixels_mut(&mut self) -> impl Iterator<Item = &mut STORAGE::Pixel>
    where
        STORAGE: StorageMut,
    {
        let view_width = self.width();
        let offset = self.offset;
        let texture_width = self.texture_size.x;

        self.storage
            .chunks_exact_mut(texture_width)
            .skip(offset.y)
            .take(self.size.y)
            .flat_map(move |row| row[offset.x..offset.x + view_width].iter_mut())
    }

    #[inline]
    pub fn iter_pixels_indexed_mut(
        &mut self,
    ) -> impl Iterator<Item = (Point2<usize>, &mut STORAGE::Pixel)>
    where
        STORAGE: StorageMut,
    {
        let view_width = self.width();
        let offset = self.offset;
        let texture_width = self.texture_size.x;

        self.storage
            .chunks_exact_mut(texture_width)
            .skip(offset.y)
            .take(self.size.y)
            .enumerate()
            .flat_map(move |(y, row)| {
                row[offset.x..offset.x + view_width]
                    .iter_mut()
                    .enumerate()
                    .map(move |(x, pixel)| (Point2::new(x, y), pixel))
            })
    }

    #[inline]
    pub fn par_rows(&self) -> impl IndexedParallelIterator<Item = &[STORAGE::Pixel]>
    where
        Self: Sync,
        STORAGE::Pixel: Send + Sync,
    {
        self.storage
            .par_chunks_exact(self.stride())
            .skip(self.offset.y)
            .take(self.height())
            .map(move |row| {
                &row[RangeInclusive::from_start_and_length(self.offset.x, self.width())]
            })
    }

    #[inline]
    pub fn par_rows_mut(&mut self) -> impl IndexedParallelIterator<Item = &mut [STORAGE::Pixel]>
    where
        Self: Sync,
        STORAGE: StorageMut,
        STORAGE::Pixel: Send + Sync,
    {
        let stride = self.stride();
        let offset = self.offset;
        let width = self.width();
        let height = self.height();

        self.storage
            .par_chunks_exact_mut(stride)
            .skip(self.offset.y)
            .take(height)
            .map(move |row| &mut row[RangeInclusive::from_start_and_length(offset.x, width)])
    }

    /// Try to produce a slice of the storage backing this texture.
    ///
    /// Returns `None` if the texture has stride
    #[inline]
    pub fn as_slice(&self) -> Option<&[STORAGE::Pixel]> {
        if self.size.x == self.texture_size.x {
            let slice = &self.storage[RangeInclusive::from_start_and_length(
                self.offset.y * self.texture_size.x,
                self.size.y * self.texture_size.x,
            )];

            Some(slice)
        } else {
            None
        }
    }

    /// Try to produce a mutable slice of the storage backing this texture.
    ///
    /// Returns `None` if the texture has stride
    #[inline]
    pub fn as_slice_mut(&mut self) -> Option<&mut [STORAGE::Pixel]>
    where
        STORAGE: StorageMut,
    {
        if self.size.x == self.texture_size.x {
            let slice = &mut self.storage[RangeInclusive::from_start_and_length(
                self.offset.y * self.texture_size.x,
                self.size.y * self.texture_size.x,
            )];

            Some(slice)
        } else {
            None
        }
    }

    #[inline]
    pub fn cast<T2: NoUninit + AnyBitPattern>(&self) -> Texture<&[T2]>
    where
        STORAGE::Pixel: NoUninit + AnyBitPattern,
    {
        assert_eq!(size_of::<STORAGE::Pixel>(), size_of::<T2>());

        Texture::from_storage_with_stride(
            self.width(),
            self.height(),
            self.stride(),
            bytemuck::cast_slice(&self.storage[..]),
        )
    }

    #[inline]
    pub fn cast_mut<T2: NoUninit + AnyBitPattern>(&mut self) -> Texture<&mut [T2]>
    where
        STORAGE: StorageMut,
        STORAGE::Pixel: NoUninit + AnyBitPattern,
    {
        assert_eq!(size_of::<STORAGE::Pixel>(), size_of::<T2>());

        Texture::from_storage_with_stride(
            self.width(),
            self.height(),
            self.stride(),
            bytemuck::cast_slice_mut(&mut self.storage[..]),
        )
    }
}

impl<STORAGE: Storage, P: Into<Point2<usize>>> Index<P> for Texture<STORAGE> {
    type Output = STORAGE::Pixel;

    #[inline]
    fn index(&self, point: P) -> &Self::Output {
        let point = point.into();

        assert!(point.x < self.size.x);
        assert!(point.y < self.size.y);

        unsafe { self.get_unchecked(point) }
    }
}

impl<STORAGE: StorageMut, P: Into<Point2<usize>>> IndexMut<P> for Texture<STORAGE> {
    #[inline]
    fn index_mut(&mut self, point: P) -> &mut Self::Output {
        let point = point.into();

        assert!(point.x < self.size.x);
        assert!(point.y < self.size.y);

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

pub trait AsTexture<P> {
    fn as_texture(&self) -> RefTexture<'_, P>;
}
impl<P, STORAGE: Storage<Pixel = P>> AsTexture<P> for Texture<STORAGE> {
    fn as_texture(&self) -> RefTexture<'_, P> {
        self.view(.., ..)
    }
}
impl<P, STORAGE: Storage<Pixel = P>> AsTexture<P> for &Texture<STORAGE> {
    fn as_texture(&self) -> RefTexture<'_, P> {
        self.view(.., ..)
    }
}
impl<P, STORAGE: Storage<Pixel = P>> AsTexture<P> for &mut Texture<STORAGE> {
    fn as_texture(&self) -> RefTexture<'_, P> {
        self.view(.., ..)
    }
}

pub trait AsTextureMut<P>: AsTexture<P> {
    fn as_texture_mut(&mut self) -> RefMutTexture<'_, P>;
}
impl<P, STORAGE: StorageMut<Pixel = P>> AsTextureMut<P> for Texture<STORAGE> {
    fn as_texture_mut(&mut self) -> RefMutTexture<'_, P> {
        self.view_mut(.., ..)
    }
}
impl<P, STORAGE: StorageMut<Pixel = P>> AsTextureMut<P> for &mut Texture<STORAGE> {
    fn as_texture_mut(&mut self) -> RefMutTexture<'_, P> {
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
