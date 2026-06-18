use alloc::{vec, vec::Vec};
use core::ops::{Index, IndexMut, RangeBounds};

use nalgebra::{Point2, Vector2};
use serde::{Deserialize, Serialize};

use crate::api::software::texture::{
    AsTextureView, AsTextureViewMut, TextureImpl, TextureImplMut, TextureView, TextureViewMut,
    resolve_range,
};

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

impl<T: 'static> AsTextureView<T> for Texture<T> {
    #[inline]
    fn as_texture_view(&self) -> TextureView<'_, T> {
        TextureView {
            texture: &self.data,
            texture_size: self.size,
            offset: Point2::new(0, 0),
            size: self.size,
        }
    }
}

impl<T: 'static> AsTextureViewMut<T> for Texture<T> {
    #[inline]
    fn as_texture_view_mut(&mut self) -> TextureViewMut<'_, T> {
        TextureViewMut {
            texture: &mut self.data,
            texture_size: self.size,
            offset: Point2::new(0, 0),
            size: self.size,
        }
    }
}

impl<T: Sized + 'static> TextureImplMut<T> for Texture<T> {
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
