use core::ops::{Mul, RangeInclusive};

use nalgebra::{ClosedAddAssign, ClosedSubAssign, Point2, Scalar, Vector2};
use num::Zero;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, Clone, Copy, PartialEq, Eq, PartialOrd)]
pub struct Rectangle<T: Scalar> {
    pub min: Point2<T>,
    pub size: Vector2<T>,
}

impl<T: Scalar> Rectangle<T> {
    pub fn from_size(size: Vector2<T>) -> Self
    where
        T: Zero,
    {
        Self {
            min: Point2::origin(),
            size,
        }
    }

    pub fn from_min_and_size(min: Point2<T>, size: Vector2<T>) -> Self {
        Self { min, size }
    }

    pub fn from_min_and_max(min: Point2<T>, max: Point2<T>) -> Self
    where
        T: ClosedSubAssign,
    {
        Self {
            size: max - min.clone(),
            min,
        }
    }

    pub fn max(&self) -> Point2<T>
    where
        T: ClosedAddAssign,
    {
        self.min.clone() + self.size.clone()
    }

    pub fn width(&self) -> T {
        self.size.x.clone()
    }

    pub fn height(&self) -> T {
        self.size.y.clone()
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

impl<T: Scalar + ClosedSubAssign + PartialOrd> TryFrom<(RangeInclusive<T>, RangeInclusive<T>)>
    for Rectangle<T>
{
    type Error = ();

    fn try_from(
        (x_range, y_range): (RangeInclusive<T>, RangeInclusive<T>),
    ) -> Result<Self, Self::Error> {
        if x_range.is_empty() || y_range.is_empty() {
            return Err(());
        }

        Ok(Self::from_min_and_max(
            Point2::new(x_range.start().clone(), y_range.start().clone()),
            Point2::new(x_range.end().clone(), y_range.end().clone()),
        ))
    }
}
