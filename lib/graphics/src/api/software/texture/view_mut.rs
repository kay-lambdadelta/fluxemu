use core::ops::{Index, IndexMut, RangeBounds, RangeInclusive};

use fluxemu_range::ContiguousRange;
use nalgebra::{Point2, Vector2};
use rayon::{
    iter::{IndexedParallelIterator, ParallelIterator},
    slice::{ParallelSlice, ParallelSliceMut},
};

use crate::api::software::texture::{
    AsTextureView, AsTextureViewMut, TextureImpl, TextureImplMut, TextureView, resolve_range,
};

#[derive(Debug, PartialEq, Eq)]
pub struct TextureViewMut<'a, T> {
    pub(super) texture: &'a mut [T],
    pub(super) texture_size: Vector2<usize>,
    pub(super) offset: Point2<usize>,
    pub(super) size: Vector2<usize>,
}

impl<'a, T> TextureViewMut<'a, T> {
    #[inline]
    pub fn from_slice(texture: &'a mut [T], width: usize, height: usize) -> Self {
        Self::from_slice_with_stride(texture, width, height, width)
    }

    #[inline]
    pub fn from_slice_with_stride(
        texture: &'a mut [T],
        width: usize,
        height: usize,
        stride: usize,
    ) -> Self {
        assert!(stride >= width, "stride must be >= width");
        assert!(
            texture.len() >= stride * height,
            "slice is too small ({}) for the given height ({}) and stride ({})",
            texture.len(),
            height,
            stride
        );

        Self {
            texture,
            texture_size: Vector2::new(stride, height),
            offset: Point2::new(0, 0),
            size: Vector2::new(width, height),
        }
    }

    #[inline]
    pub fn stride(&self) -> usize {
        self.texture_size.x
    }
}

impl<T: Sized> AsTextureView<T> for TextureViewMut<'_, T> {
    #[inline]
    fn as_texture_view(&self) -> TextureView<'_, T> {
        TextureView {
            texture: self.texture,
            texture_size: self.texture_size,
            offset: self.offset,
            size: self.size,
        }
    }
}

impl<T: Sized> AsTextureViewMut<T> for TextureViewMut<'_, T> {
    #[inline]
    fn as_texture_view_mut(&mut self) -> TextureViewMut<'_, T> {
        TextureViewMut {
            texture: self.texture,
            texture_size: self.texture_size,
            offset: self.offset,
            size: self.size,
        }
    }
}

impl<'a, P: Into<Point2<usize>>, T: 'static> Index<P> for TextureViewMut<'a, T> {
    type Output = T;

    #[inline]
    fn index(&self, point: P) -> &Self::Output {
        let point = point.into();

        assert!(point.x < self.size.x);
        assert!(point.y < self.size.y);

        unsafe { self.get_unchecked(point) }
    }
}

impl<'a, P: Into<Point2<usize>>, T: 'static> IndexMut<P> for TextureViewMut<'a, T> {
    #[inline]
    fn index_mut(&mut self, point: P) -> &mut Self::Output {
        let point = point.into();

        assert!(point.x < self.size.x);
        assert!(point.y < self.size.y);

        unsafe { self.get_unchecked_mut(point) }
    }
}

impl<'a, T: Sized + 'static> TextureImpl<T> for TextureViewMut<'a, T> {
    #[inline]
    fn width(&self) -> usize {
        self.size.x
    }

    #[inline]
    fn height(&self) -> usize {
        self.size.y
    }

    #[inline]
    fn view(&self, x: impl RangeBounds<usize>, y: impl RangeBounds<usize>) -> TextureView<'_, T> {
        let (x0, x1) = resolve_range(x, self.width());
        let (y0, y1) = resolve_range(y, self.height());

        let start = Point2::new(x0, y0);
        let end = Point2::new(x1, y1);

        TextureView {
            texture: self.texture,
            texture_size: self.texture_size,
            offset: start + self.offset.coords,
            size: end - start,
        }
    }

    #[inline]
    unsafe fn get_unchecked(&self, point: impl Into<Point2<usize>>) -> &T {
        let point = point.into();

        debug_assert!(point.x < self.width());
        debug_assert!(point.y < self.height());

        let global = point + self.offset.coords;
        let index = global.y * self.texture_size.x + global.x;

        unsafe { self.texture.get_unchecked(index) }
    }

    fn par_rows(&self) -> impl IndexedParallelIterator<Item = &[T]>
    where
        Self: Sync,
        T: Send + Sync,
    {
        self.texture
            .par_chunks_exact(self.stride())
            .skip(self.offset.y)
            .take(self.height())
            .map(move |row| {
                &row[RangeInclusive::from_start_and_length(self.offset.x, self.width())]
            })
    }
}

impl<'a, T: Sized + 'static> TextureImplMut<T> for TextureViewMut<'a, T> {
    #[inline]
    fn view_mut(
        &mut self,
        x: impl RangeBounds<usize>,
        y: impl RangeBounds<usize>,
    ) -> TextureViewMut<'_, T> {
        let (x0, x1) = resolve_range(x, self.width());
        let (y0, y1) = resolve_range(y, self.height());

        let start = Point2::new(x0, y0);
        let end = Point2::new(x1, y1);

        TextureViewMut {
            texture: self.texture,
            texture_size: self.texture_size,
            offset: start + self.offset.coords,
            size: end - start,
        }
    }

    #[inline]
    fn iter_pixels_mut(&mut self) -> impl Iterator<Item = &mut T> {
        let view_width = self.width();
        let offset = self.offset;
        let texture_width = self.texture_size.x;

        self.texture
            .chunks_exact_mut(texture_width)
            .skip(offset.y)
            .take(self.size.y)
            .flat_map(move |row| row[offset.x..offset.x + view_width].iter_mut())
    }

    #[inline]
    fn iter_pixels_indexed_mut(&mut self) -> impl Iterator<Item = (Point2<usize>, &mut T)> {
        let view_width = self.width();
        let offset = self.offset;
        let texture_width = self.texture_size.x;

        self.texture
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
    fn fill(&mut self, value: T)
    where
        T: Clone,
    {
        if self.offset.x == 0 && self.size.x == self.texture_size.x {
            let range = RangeInclusive::from_start_and_length(
                self.offset.y * self.texture_size.x,
                self.size.y * self.texture_size.x,
            );

            self.texture[range].fill(value);
        } else {
            let x_range = RangeInclusive::from_start_and_length(self.offset.x, self.size.x);

            for row in self
                .texture
                .chunks_exact_mut(self.texture_size.x)
                .skip(self.offset.y)
                .take(self.size.y)
            {
                row[x_range.clone()].fill(value.clone());
            }
        }
    }

    #[inline]
    unsafe fn get_unchecked_mut(&mut self, point: impl Into<Point2<usize>>) -> &mut T {
        let point = point.into();

        debug_assert!(point.x < self.width());
        debug_assert!(point.y < self.height());

        let global = point + self.offset.coords;
        let index = global.y * self.texture_size.x + global.x;

        unsafe { self.texture.get_unchecked_mut(index) }
    }

    #[inline]
    fn par_rows_mut(&mut self) -> impl IndexedParallelIterator<Item = &mut [T]>
    where
        Self: Sync,
        T: Send + Sync,
    {
        let stride = self.stride();
        let offset = self.offset;
        let width = self.width();
        let height = self.height();

        self.texture
            .par_chunks_exact_mut(stride)
            .skip(self.offset.y)
            .take(height)
            .map(move |row| &mut row[RangeInclusive::from_start_and_length(offset.x, width)])
    }
}
