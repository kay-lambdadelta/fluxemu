use core::range::RangeInclusive;

use num::{Integer, ToPrimitive};
use rangemap::{RangeInclusiveSet, StepLite};

use crate::{ContiguousRange, RangeBase, RangeDifference, RangeIntersection};

impl<Idx: Integer + Clone> RangeBase<Idx> for RangeInclusive<Idx> {
    #[inline]
    fn is_empty(&self) -> bool {
        self.is_empty()
    }

    #[inline]
    fn contains(&self, index: &Idx) -> bool {
        RangeInclusive::contains(self, index)
    }
}

impl<Idx: Integer + Clone + ToPrimitive> ContiguousRange<Idx> for RangeInclusive<Idx> {
    #[inline]
    fn from_start_and_length(start: Idx, length: Idx) -> Self {
        let one = Idx::one();

        RangeInclusive {
            start: start.clone(),
            last: start + length - one,
        }
    }

    #[inline]
    fn is_adjacent(&self, other: &Self) -> bool {
        !self.is_empty()
            && !other.is_empty()
            && (self.last.clone() + Idx::one() == other.start.clone()
                || other.last.clone() + Idx::one() == self.start.clone())
    }

    #[inline]
    fn len(&self) -> usize {
        if self.is_empty() {
            return 0;
        }

        let start = self.start.to_usize().unwrap();
        let last = self.last.to_usize().unwrap();

        last - start + 1
    }
}

impl<Idx: Integer + Clone> RangeIntersection<Idx, Self> for RangeInclusive<Idx> {
    type Output = RangeInclusive<Idx>;

    #[inline]
    fn intersection(&self, rhs: &Self) -> Self::Output {
        let start = core::cmp::max(&self.start, &rhs.start).clone();
        let last = core::cmp::min(&self.last, &rhs.last).clone();

        RangeInclusive { start, last }
    }

    #[inline]
    fn intersects(&self, rhs: &Self) -> bool {
        !self.intersection(rhs).is_empty()
    }
}

impl<Idx: Integer + Clone + StepLite> RangeDifference<Idx, Self> for RangeInclusive<Idx> {
    type Output = RangeInclusiveSet<Idx>;

    #[inline]
    fn difference(&self, rhs: &Self) -> Self::Output {
        if self.is_empty() {
            return RangeInclusiveSet::default();
        }

        if rhs.last < self.start || rhs.start > self.last {
            return RangeInclusiveSet::from_iter([self.clone().into()]);
        }

        let mut result = RangeInclusiveSet::default();
        let one = Idx::one();

        if rhs.start > self.start {
            let left_last = rhs.start.clone() - one.clone();

            if left_last >= self.start {
                result.insert(self.start.clone()..=left_last);
            }
        }

        if rhs.last < self.last {
            let right_start = rhs.last.clone() + one;

            if right_start <= self.last {
                result.insert(right_start..=self.last.clone());
            }
        }

        result
    }
}
