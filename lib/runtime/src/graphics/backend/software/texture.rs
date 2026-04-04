use std::ops::{Bound, Index, IndexMut, RangeBounds};

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

    #[inline]
    fn get(&self, point: impl Into<Point2<usize>>) -> Option<&T> {
        let point = point.into();

        if point.x < self.width() && point.y < self.height() {
            Some(&self[point])
        } else {
            None
        }
    }

    fn as_view(&'_ self) -> TextureView<'_, T>;
    fn view(&self, x: impl RangeBounds<usize>, y: impl RangeBounds<usize>) -> TextureView<'_, T>;

    fn iter_pixels(&self, mut callback: impl FnMut(Point2<usize>, &T)) {
        for y in 0..self.height() {
            for x in 0..self.width() {
                let point = Point2::new(x, y);

                callback(point, &self[point]);
            }
        }
    }
}

pub trait TextureImplMut<T: Sized>: TextureImpl<T> + IndexMut<Point2<usize>> {
    #[inline]
    fn get_mut(&mut self, point: impl Into<Point2<usize>>) -> Option<&mut T> {
        let point = point.into();

        if point.x < self.width() && point.y < self.height() {
            Some(&mut self[point])
        } else {
            None
        }
    }

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

        for y in 0..self.height() {
            for x in 0..self.width() {
                let index = Point2::new(x, y);

                let source_pixel = match mode {
                    CopyMode::Nearest => {
                        let source_coordinates = index
                            .coords
                            .component_mul(&other.size())
                            .component_div(&self.size());

                        let clamped_source_coords = source_coordinates
                            .zip_map(&other.size(), |a: usize, b| a.min(b.saturating_sub(1)));

                        other[clamped_source_coords.into()].clone()
                    }
                };

                self[index] = source_pixel.into();
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

    fn iter_pixels_mut(&mut self, mut callback: impl FnMut(Point2<usize>, &mut T)) {
        for y in 0..self.height() {
            for x in 0..self.width() {
                let point = Point2::new(x, y);

                callback(point, &mut self[point]);
            }
        }
    }
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
}

impl<P: Into<Point2<usize>>, T> Index<P> for Texture<T> {
    type Output = T;

    #[inline]
    fn index(&self, point: P) -> &Self::Output {
        let point = point.into();
        let index = point.y * self.size.x + point.x;

        &self.data[index]
    }
}

impl<P: Into<Point2<usize>>, T> IndexMut<P> for Texture<T> {
    #[inline]
    fn index_mut(&mut self, point: P) -> &mut Self::Output {
        let point = point.into();
        let index = point.y * self.size.x + point.x;

        &mut self.data[index]
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

        let global = point + self.offset.coords;

        let index = global.y * self.texture_size.x + global.x;
        &self.texture[index]
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

        let global = point + self.offset.coords;

        let index = global.y * self.texture_size.x + global.x;
        &self.texture[index]
    }
}

impl<'a, P: Into<Point2<usize>>, T> IndexMut<P> for TextureViewMut<'a, T> {
    #[inline]
    fn index_mut(&mut self, point: P) -> &mut Self::Output {
        let point = point.into();

        assert!(point.x < self.size.x);
        assert!(point.y < self.size.y);

        let global = point + self.offset.coords;

        let index = global.y * self.texture_size.x + global.x;
        &mut self.texture[index]
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
