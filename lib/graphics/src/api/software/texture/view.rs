use core::ops::{Index, RangeBounds};

use nalgebra::{Point2, Vector2};

use crate::api::software::texture::{AsTextureView, TextureImpl, resolve_range};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TextureView<'a, T> {
    pub(super) texture: &'a [T],
    pub(super) texture_size: Vector2<usize>,
    pub(super) offset: Point2<usize>,
    pub(super) size: Vector2<usize>,
}

impl<'a, T> TextureView<'a, T> {
    #[inline]
    pub fn from_slice(texture: &'a [T], width: usize, height: usize) -> Self {
        Self::from_slice_with_stride(texture, width, height, width)
    }

    #[inline]
    pub fn from_slice_with_stride(
        texture: &'a [T],
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

impl<T: Sized> AsTextureView<T> for TextureView<'_, T> {
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
