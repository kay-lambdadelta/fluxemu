use alloc::{vec, vec::Vec};
use core::ops::{Bound, Index, IndexMut, RangeBounds, RangeInclusive};

use fluxemu_range::ContiguousRange;
use itertools::Itertools;
use nalgebra::{Point2, Vector2};
use serde::{Deserialize, Serialize};

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum CopyMode {
    Nearest,
}

pub trait TextureImpl<T: Sized>: Index<Point2<usize>, Output = T> + Sized {
    fn width(&self) -> usize;
    fn height(&self) -> usize;

    #[inline]
    fn size(&self) -> Vector2<usize> {
        Vector2::new(self.width(), self.height())
    }

    fn as_view(&'_ self) -> TextureView<'_, T>;
    fn view(&self, x: impl RangeBounds<usize>, y: impl RangeBounds<usize>) -> TextureView<'_, T>;

    #[inline]
    fn iter_pixels<'a>(&'a self) -> impl Iterator<Item = &'a T> + 'a
    where
        T: 'a,
    {
        (0..self.height())
            .cartesian_product(0..self.width())
            .map(|(y, x)| {
                let point = Point2::new(x, y);
                &self[point]
            })
    }

    #[inline]
    fn iter_pixels_indexed<'a>(&'a self) -> impl Iterator<Item = (Point2<usize>, &'a T)> + 'a
    where
        T: 'a,
    {
        (0..self.height())
            .cartesian_product(0..self.width())
            .map(|(y, x)| {
                let point = Point2::new(x, y);

                (point, &self[point])
            })
    }

    ///  # Safety
    ///     Access must not be out of bounds
    unsafe fn get_unchecked(&self, point: impl Into<Point2<usize>>) -> &T;
}

pub trait TextureImplMut<T: Sized>: TextureImpl<T> + IndexMut<Point2<usize>> {
    fn as_view_mut(&'_ mut self) -> TextureViewMut<'_, T>;

    #[inline]
    fn fill(&mut self, value: T)
    where
        T: Clone,
    {
        for y in 0..self.height() {
            for x in 0..self.width() {
                let point = Point2::new(x, y);

                self[point] = value.clone();
            }
        }
    }

    fn view_mut(
        &mut self,
        x: impl RangeBounds<usize>,
        y: impl RangeBounds<usize>,
    ) -> TextureViewMut<'_, T>;

    #[inline]
    fn copy_from<T2: Into<T> + Clone>(&mut self, other: &impl TextureImpl<T2>, mode: CopyMode) {
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
    fn flip_x(&mut self)
    where
        T: Clone,
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
    fn flip_y(&mut self)
    where
        T: Clone,
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

    fn iter_pixels_mut<'a>(&'a mut self) -> impl Iterator<Item = &'a mut T> + 'a
    where
        T: 'a;

    fn iter_pixels_indexed_mut<'a>(
        &'a mut self,
    ) -> impl Iterator<Item = (Point2<usize>, &'a mut T)> + 'a
    where
        T: 'a;

    ///  # Safety
    ///     Access must not be out of bounds
    unsafe fn get_unchecked_mut(&mut self, point: impl Into<Point2<usize>>) -> &mut T;
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Texture<T> {
    data: Vec<T>,
    size: Vector2<usize>,
}

impl<T: Sized + 'static> Texture<T> {
    #[inline]
    pub fn new(width: usize, height: usize, data: T) -> Self
    where
        T: Clone,
    {
        let len = width * height;

        Self {
            data: vec![data; len],
            size: Vector2::new(width, height),
        }
    }

    #[inline]
    pub fn from_fn(
        width: usize,
        height: usize,
        mut producer: impl FnMut(usize, usize) -> T,
    ) -> Self {
        let len = width * height;

        Self {
            data: (0..len).map(|i| producer(i % width, i / width)).collect(),
            size: Vector2::new(width, height),
        }
    }

    #[inline]
    pub fn from_vec(width: usize, height: usize, data: Vec<T>) -> Self {
        let size = Vector2::new(width, height);
        assert_eq!(data.len(), size.product());

        Self { data, size }
    }

    #[inline]
    pub fn as_slice(&self) -> &[T] {
        &self.data
    }

    #[inline]
    pub fn as_mut_slice(&mut self) -> &mut [T] {
        &mut self.data
    }
}

impl<T: Sized + 'static> TextureImpl<T> for Texture<T> {
    #[inline]
    fn width(&self) -> usize {
        self.size.x
    }

    #[inline]
    fn height(&self) -> usize {
        self.size.y
    }

    #[inline]
    fn as_view(&'_ self) -> TextureView<'_, T> {
        TextureView {
            texture: &self.data,
            texture_size: self.size,
            offset: Point2::new(0, 0),
            size: self.size,
        }
    }

    #[inline]
    fn view(&self, x: impl RangeBounds<usize>, y: impl RangeBounds<usize>) -> TextureView<'_, T> {
        let (x0, x1) = resolve_range(x, self.width());
        let (y0, y1) = resolve_range(y, self.height());

        let start = Point2::new(x0, y0);
        let end = Point2::new(x1, y1);

        TextureView {
            texture: &self.data,
            texture_size: self.size,
            offset: start,
            size: end - start,
        }
    }

    #[inline]
    unsafe fn get_unchecked(&self, point: impl Into<Point2<usize>>) -> &T {
        let point = point.into();

        debug_assert!(point.x < self.width());
        debug_assert!(point.y < self.height());

        let index = point.y * self.size.x + point.x;

        unsafe { self.data.get_unchecked(index) }
    }
}

impl<P: Into<Point2<usize>>, T: 'static> Index<P> for Texture<T> {
    type Output = T;

    #[inline]
    fn index(&self, point: P) -> &Self::Output {
        let point = point.into();

        assert!(point.x < self.width());
        assert!(point.y < self.height());

        unsafe { self.get_unchecked(point) }
    }
}

impl<P: Into<Point2<usize>>, T: 'static> IndexMut<P> for Texture<T> {
    #[inline]
    fn index_mut(&mut self, point: P) -> &mut Self::Output {
        let point = point.into();

        assert!(point.x < self.width());
        assert!(point.y < self.height());

        unsafe { self.get_unchecked_mut(point) }
    }
}

impl<T: Sized + 'static> TextureImplMut<T> for Texture<T> {
    #[inline]
    fn as_view_mut(&'_ mut self) -> TextureViewMut<'_, T> {
        TextureViewMut {
            texture: &mut self.data,
            texture_size: self.size,
            offset: Point2::new(0, 0),
            size: self.size,
        }
    }

    #[inline]
    fn fill(&mut self, value: T)
    where
        T: Clone,
    {
        self.as_mut_slice().fill(value);
    }

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
            texture: &mut self.data,
            texture_size: self.size,
            offset: start,
            size: end - start,
        }
    }

    #[inline]
    fn iter_pixels_mut<'a>(&'a mut self) -> impl Iterator<Item = &'a mut T> + 'a
    where
        T: 'a,
    {
        self.data.iter_mut()
    }

    #[inline]
    fn iter_pixels_indexed_mut<'a>(
        &'a mut self,
    ) -> impl Iterator<Item = (Point2<usize>, &'a mut T)> + 'a
    where
        T: 'a,
    {
        let width = self.width();

        self.data
            .iter_mut()
            .enumerate()
            .map(move |(i, pixel)| (Point2::new(i % width, i / width), pixel))
    }

    #[inline]
    unsafe fn get_unchecked_mut(&mut self, point: impl Into<Point2<usize>>) -> &mut T {
        let point = point.into();

        debug_assert!(point.x < self.width());
        debug_assert!(point.y < self.height());

        let index = point.y * self.size.x + point.x;

        unsafe { self.data.get_unchecked_mut(index) }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TextureView<'a, T> {
    texture: &'a [T],
    texture_size: Vector2<usize>,
    offset: Point2<usize>,
    size: Vector2<usize>,
}

impl<'a, T> TextureView<'a, T> {
    #[inline]
    pub fn from_slice(texture: &'a [T], width: usize, height: usize) -> Self {
        assert_eq!(texture.len(), width * height);

        Self {
            texture,
            texture_size: Vector2::new(width, height),
            offset: Point2::new(0, 0),
            size: Vector2::new(width, height),
        }
    }
}

impl<'a, P: Into<Point2<usize>>, T> Index<P> for TextureView<'a, T> {
    type Output = T;

    #[inline]
    fn index(&self, point: P) -> &Self::Output {
        let point = point.into();

        assert!(point.x < self.size.x);
        assert!(point.y < self.size.y);

        unsafe { self.get_unchecked(point) }
    }
}

impl<'a, T: Sized> TextureImpl<T> for TextureView<'a, T> {
    #[inline]
    fn width(&self) -> usize {
        self.size.x
    }

    #[inline]
    fn height(&self) -> usize {
        self.size.y
    }

    #[inline]
    fn as_view(&'_ self) -> TextureView<'_, T> {
        Self {
            texture: self.texture,
            texture_size: self.texture_size,
            offset: self.offset,
            size: self.size,
        }
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

    unsafe fn get_unchecked(&self, point: impl Into<Point2<usize>>) -> &T {
        let point = point.into();

        debug_assert!(point.x < self.width());
        debug_assert!(point.y < self.height());

        let global = point + self.offset.coords;
        let index = global.y * self.texture_size.x + global.x;

        unsafe { self.texture.get_unchecked(index) }
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct TextureViewMut<'a, T> {
    texture: &'a mut [T],
    texture_size: Vector2<usize>,
    offset: Point2<usize>,
    size: Vector2<usize>,
}

impl<'a, T> TextureViewMut<'a, T> {
    #[inline]
    pub fn from_slice(texture: &'a mut [T], width: usize, height: usize) -> Self {
        assert_eq!(texture.len(), width * height);

        let size = Vector2::new(width, height);

        Self {
            texture,
            texture_size: size,
            offset: Point2::new(0, 0),
            size,
        }
    }
}

impl<'a, P: Into<Point2<usize>>, T> Index<P> for TextureViewMut<'a, T> {
    type Output = T;

    #[inline]
    fn index(&self, point: P) -> &Self::Output {
        let point = point.into();

        assert!(point.x < self.size.x);
        assert!(point.y < self.size.y);

        unsafe { self.get_unchecked(point) }
    }
}

impl<'a, P: Into<Point2<usize>>, T> IndexMut<P> for TextureViewMut<'a, T> {
    #[inline]
    fn index_mut(&mut self, point: P) -> &mut Self::Output {
        let point = point.into();

        assert!(point.x < self.size.x);
        assert!(point.y < self.size.y);

        unsafe { self.get_unchecked_mut(point) }
    }
}

impl<'a, T: Sized> TextureImpl<T> for TextureViewMut<'a, T> {
    #[inline]
    fn width(&self) -> usize {
        self.size.x
    }

    #[inline]
    fn height(&self) -> usize {
        self.size.y
    }

    #[inline]
    fn as_view(&'_ self) -> TextureView<'_, T> {
        TextureView {
            texture: self.texture,
            texture_size: self.texture_size,
            offset: self.offset,
            size: self.size,
        }
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
}

impl<'a, T: Sized> TextureImplMut<T> for TextureViewMut<'a, T> {
    #[inline]
    fn as_view_mut(&'_ mut self) -> TextureViewMut<'_, T> {
        TextureViewMut {
            texture: self.texture,
            texture_size: self.texture_size,
            offset: self.offset,
            size: self.size,
        }
    }

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
    fn iter_pixels_mut<'b>(&'b mut self) -> impl Iterator<Item = &'b mut T> + 'b
    where
        T: 'b,
    {
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
    fn iter_pixels_indexed_mut<'b>(
        &'b mut self,
    ) -> impl Iterator<Item = (Point2<usize>, &'b mut T)> + 'b
    where
        T: 'b,
    {
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
