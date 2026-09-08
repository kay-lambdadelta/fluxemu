use core::ops::{Mul, RangeInclusive};

use nalgebra::{ClosedAddAssign, ClosedSubAssign, Point2, Scalar, Vector2};
use num::Zero;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, Clone, Copy, PartialEq, Eq, PartialOrd)]
pub struct Rectangle<T: Scalar> {
    pub min: Point2<T>,
    pub max: Point2<T>,
}

impl<T: Scalar> Rectangle<T> {
    pub fn from_min_and_size(min: Point2<T>, size: Vector2<T>) -> Self
    where
        T: ClosedAddAssign + Clone,
    {
        Self {
            max: min.clone() + size,
            min,
        }
    }

    pub fn from_size(size: Vector2<T>) -> Self
    where
        T: ClosedAddAssign + Zero,
    {
        Self::from_min_and_size(Point2::origin(), size)
    }

    pub fn from_min_and_max(min: Point2<T>, max: Point2<T>) -> Self {
        Self { min, max }
    }

    pub fn from_range(x: RangeInclusive<T>, y: RangeInclusive<T>) -> Self {
        Self::from_min_and_max(
            Point2::new(x.start().clone(), y.start().clone()),
            Point2::new(x.end().clone(), y.end().clone()),
        )
    }

    pub fn width(&self) -> T
    where
        T: ClosedSubAssign + Clone,
    {
        self.size().x.clone()
    }

    pub fn height(&self) -> T
    where
        T: ClosedSubAssign + Clone,
    {
        self.size().y.clone()
    }

    pub fn size(&self) -> Vector2<T>
    where
        T: ClosedSubAssign + Clone,
    {
        self.max.clone() - self.min.clone()
    }

    pub fn area(&self) -> T
    where
        T: ClosedSubAssign + Clone + Mul<Output = T>,
    {
        self.width() * self.height()
    }

    pub fn is_square(&self) -> bool
    where
        T: ClosedSubAssign + Clone,
    {
        self.width() == self.height()
    }

    pub fn is_valid(&self) -> bool
    where
        T: ClosedSubAssign + PartialOrd,
    {
        self.min.x <= self.max.x && self.min.y <= self.max.y
    }

    pub fn overlaps(&self, other: &Self) -> bool
    where
        T: ClosedAddAssign + PartialOrd,
    {
        self.max.x >= other.min.x
            && self.min.x <= other.max.x
            && self.max.y >= other.min.y
            && self.min.y <= other.max.y
    }

    pub fn x_range(&self) -> RangeInclusive<T>
    where
        T: ClosedAddAssign + Clone,
    {
        self.min.x.clone()..=self.max.x.clone()
    }

    pub fn y_range(&self) -> RangeInclusive<T>
    where
        T: ClosedAddAssign + Clone,
    {
        self.min.y.clone()..=self.max.y.clone()
    }
}
