use core::ops::{Bound, Index, IndexMut, RangeBounds};

use itertools::Itertools;
use nalgebra::{Point2, Vector2};

mod owned;
pub use owned::Texture;
mod view;
use rayon::iter::IndexedParallelIterator;
pub use view::TextureView;
mod view_mut;
pub use view_mut::TextureViewMut;

pub trait TextureImpl<T: Sized + 'static>:
    Index<Point2<usize>, Output = T> + AsTextureView<T> + Sized
{
    ///  # Safety
    ///     Access must not be out of bounds
    unsafe fn get_unchecked(&self, point: impl Into<Point2<usize>>) -> &T;

    fn width(&self) -> usize;
    fn height(&self) -> usize;

    #[inline]
    fn size(&self) -> Vector2<usize> {
        Vector2::new(self.width(), self.height())
    }

    fn view(&self, x: impl RangeBounds<usize>, y: impl RangeBounds<usize>) -> TextureView<'_, T>;

    #[inline]
    fn iter_pixels(&self) -> impl Iterator<Item = &T> {
        (0..self.height())
            .cartesian_product(0..self.width())
            .map(|(y, x)| {
                let point = Point2::new(x, y);
                &self[point]
            })
    }

    #[inline]
    fn iter_pixels_indexed(&self) -> impl Iterator<Item = (Point2<usize>, &T)> {
        (0..self.height())
            .cartesian_product(0..self.width())
            .map(|(y, x)| {
                let point = Point2::new(x, y);

                (point, &self[point])
            })
    }

    fn par_rows(&self) -> impl IndexedParallelIterator<Item = &[T]>
    where
        Self: Sync,
        T: Send + Sync;
}

pub trait TextureImplMut<T: Sized + 'static>:
    TextureImpl<T> + AsTextureViewMut<T> + IndexMut<Point2<usize>>
{
    ///  # Safety
    ///     Access must not be out of bounds
    unsafe fn get_unchecked_mut(&mut self, point: impl Into<Point2<usize>>) -> &mut T;

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
    fn copy_from<T2: Into<T> + Clone + 'static>(
        &mut self,
        other: &impl TextureImpl<T2>,
        mode: CopyMode,
    ) {
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

    fn iter_pixels_mut(&mut self) -> impl Iterator<Item = &mut T>;
    fn iter_pixels_indexed_mut(&mut self) -> impl Iterator<Item = (Point2<usize>, &mut T)>;

    fn par_rows_mut(&mut self) -> impl IndexedParallelIterator<Item = &mut [T]>
    where
        Self: Sync,
        T: Send + Sync;
}

pub trait AsTextureView<T> {
    fn as_texture_view(&self) -> TextureView<'_, T>;
}

pub trait AsTextureViewMut<T>: AsTextureView<T> {
    fn as_texture_view_mut(&mut self) -> TextureViewMut<'_, T>;
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

impl<T, V: AsTextureView<T>> AsTextureView<T> for &V {
    #[inline]
    fn as_texture_view(&self) -> TextureView<'_, T> {
        (self as &V).as_texture_view()
    }
}

impl<T, V: AsTextureViewMut<T>> AsTextureView<T> for &mut V {
    #[inline]
    fn as_texture_view(&self) -> TextureView<'_, T> {
        (self as &V).as_texture_view()
    }
}

impl<T, V: AsTextureViewMut<T>> AsTextureViewMut<T> for &mut V {
    #[inline]
    fn as_texture_view_mut(&mut self) -> TextureViewMut<'_, T> {
        (self as &mut V).as_texture_view_mut()
    }
}
