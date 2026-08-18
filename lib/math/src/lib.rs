use std::ops::Mul;

use nalgebra::{ClosedAddAssign, ClosedSubAssign, Point2, Scalar, Vector2};
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, Clone, Copy, PartialEq, Eq, PartialOrd)]
pub struct Rectangle<T: Scalar> {
    pub start: Point2<T>,
    pub extent: Vector2<T>,
}

impl<T: Scalar> Rectangle<T> {
    pub fn from_start_and_extent(start: Point2<T>, extent: Vector2<T>) -> Self {
        Self { start, extent }
    }

    pub fn from_start_and_end(start: Point2<T>, end: Point2<T>) -> Self
    where
        T: ClosedSubAssign,
    {
        Self {
            extent: end - start.clone(),
            start,
        }
    }

    pub fn end(&self) -> Point2<T>
    where
        T: ClosedAddAssign,
    {
        self.start.clone() + self.extent.clone()
    }

    pub fn width(&self) -> T {
        self.extent.x.clone()
    }

    pub fn height(&self) -> T {
        self.extent.y.clone()
    }

    pub fn area(&self) -> T
    where
        T: Mul<Output = T>,
    {
        self.width() * self.height()
    }

    pub fn is_square(&self) -> bool {
        self.width() == self.height()
    }
}
