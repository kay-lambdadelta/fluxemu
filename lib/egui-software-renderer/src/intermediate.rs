use std::{
    cmp::Ordering,
    fmt::Debug,
    ops::{Add, AddAssign, Div, DivAssign, Mul, MulAssign, Neg, Rem, RemAssign, Sub, SubAssign},
};

use nalgebra::Point2;
use num::{
    Bounded, FromPrimitive, One, Signed, ToPrimitive, Zero,
    traits::{Euclid, NumAssignRef, NumRef},
};
use palette::{
    blend::{PreAlpha, Premultiply},
    bool_mask::HasBoolMask,
    num::{Clamp, IsValidDivisor, Real},
    stimulus::IntoStimulus,
};

pub trait Intermediate:
    NumRef
    + NumAssignRef
    + Signed
    + Bounded
    + Euclid
    + FromPrimitive
    + ToPrimitive
    + PartialOrd
    + Copy
    + Debug
    + Send
    + Sync
    + 'static
{
    #[inline]
    fn half() -> Self {
        Self::one() / (Self::one() + Self::one())
    }

    #[inline]
    fn max_int() -> Self {
        Self::max_value().floor()
    }

    #[inline]
    fn floor(self) -> Self {
        self - self.rem_euclid(&Self::one())
    }

    #[inline]
    fn ceil(self) -> Self {
        -(-self).floor()
    }

    #[inline]
    fn min(self, other: Self) -> Self {
        if other < self { other } else { self }
    }

    #[inline]
    fn max(self, other: Self) -> Self {
        if other > self { other } else { self }
    }

    #[inline]
    fn component_min(a: Point2<Self>, b: Point2<Self>) -> Point2<Self> {
        Point2::from(a.coords.zip_map(&b.coords, Self::min))
    }

    #[inline]
    fn component_max(a: Point2<Self>, b: Point2<Self>) -> Point2<Self> {
        Point2::from(a.coords.zip_map(&b.coords, Self::max))
    }

    #[inline]
    fn is_finite(self) -> bool {
        (self * Self::zero()).is_zero()
    }

    #[inline]
    fn to_index(self) -> usize {
        if self.partial_cmp(&Self::zero()) != Some(Ordering::Greater) {
            return 0;
        }

        self.to_usize().unwrap_or(usize::MAX)
    }

    #[inline]
    fn to_texel_index(self, max: u32) -> u32 {
        if self.partial_cmp(&Self::zero()) != Some(Ordering::Greater) {
            return 0;
        }

        self.to_u32().map_or(max, |value| value.min(max))
    }
}

impl<
    T: NumRef
        + NumAssignRef
        + Signed
        + Bounded
        + Euclid
        + FromPrimitive
        + ToPrimitive
        + PartialOrd
        + Copy
        + Debug
        + Send
        + Sync
        + 'static,
> Intermediate for T
{
}

/// Wrapper to deal with palette's almost, but not quite, num traits
#[derive(Copy, Clone, Debug, Default, PartialEq, PartialOrd)]
#[repr(transparent)]
pub struct Channel<T>(pub T);

macro_rules! impl_channel_operator {
    ($($trait:ident::$method:ident, $assign_trait:ident::$assign_method:ident);+ $(;)?) => {
        $(
            impl<T: Intermediate> $trait for Channel<T> {
                type Output = Self;

                #[inline(always)]
                fn $method(self, other: Self) -> Self {
                    Channel(self.0.$method(other.0))
                }
            }

            impl<'a, T: Intermediate> $trait<&'a Channel<T>> for Channel<T> {
                type Output = Self;

                #[inline(always)]
                fn $method(self, other: &'a Self) -> Self {
                    Channel(self.0.$method(other.0))
                }
            }

            impl<T: Intermediate> $assign_trait for Channel<T> {
                #[inline(always)]
                fn $assign_method(&mut self, other: Self) {
                    self.0.$assign_method(other.0);
                }
            }
        )+
    };
}

impl_channel_operator!(
    Add::add, AddAssign::add_assign;
    Sub::sub, SubAssign::sub_assign;
    Mul::mul, MulAssign::mul_assign;
    Div::div, DivAssign::div_assign;
    Rem::rem, RemAssign::rem_assign;
);

impl<C, T: Intermediate> Mul<Channel<T>> for PreAlpha<C>
where
    C: Mul<Channel<T>, Output = C> + Premultiply<Scalar = Channel<T>>,
{
    type Output = Self;

    #[inline(always)]
    fn mul(self, factor: Channel<T>) -> Self {
        PreAlpha {
            color: self.color * factor,
            alpha: self.alpha * factor,
        }
    }
}

impl<T: Intermediate> Neg for Channel<T> {
    type Output = Self;

    #[inline]
    fn neg(self) -> Self {
        Channel(-self.0)
    }
}

impl<T: Intermediate> Zero for Channel<T> {
    #[inline]
    fn zero() -> Self {
        Channel(T::zero())
    }

    #[inline]
    fn is_zero(&self) -> bool {
        self.0.is_zero()
    }
}

impl<T: Intermediate> One for Channel<T> {
    #[inline]
    fn one() -> Self {
        Channel(T::one())
    }
}

impl<T: Intermediate> palette::num::Zero for Channel<T> {
    #[inline]
    fn zero() -> Self {
        Channel(T::zero())
    }
}

impl<T: Intermediate> palette::num::One for Channel<T> {
    #[inline]
    fn one() -> Self {
        Channel(T::one())
    }
}

impl<T: Intermediate> Real for Channel<T> {
    #[inline]
    fn from_f64(value: f64) -> Self {
        Channel(T::from_f64(value).unwrap_or_else(|| {
            panic!("value is out of range for the intermediate type: {}", value)
        }))
    }
}

impl<T: Intermediate> Clamp for Channel<T> {
    #[inline]
    fn clamp(self, min: Self, max: Self) -> Self {
        self.clamp_min(min).clamp_max(max)
    }

    #[inline]
    fn clamp_min(self, min: Self) -> Self {
        Channel(self.0.max(min.0))
    }

    #[inline]
    fn clamp_max(self, max: Self) -> Self {
        Channel(self.0.min(max.0))
    }
}

impl<T: Intermediate> HasBoolMask for Channel<T> {
    type Mask = bool;
}

impl<T: Intermediate> IsValidDivisor for Channel<T> {
    #[inline]
    fn is_valid_divisor(&self) -> bool {
        !self.0.is_zero()
    }
}

impl<T: Intermediate> IntoStimulus<u8> for Channel<T> {
    #[inline]
    fn into_stimulus(self) -> u8 {
        let scaled: T = Clamp::clamp(self, Channel(T::zero()), Channel(T::one())).0
            * T::from_u8(u8::MAX).unwrap()
            + T::half();

        scaled.to_u8().unwrap_or(u8::MAX)
    }
}

impl<T: Intermediate> IntoStimulus<Channel<T>> for u8 {
    #[inline]
    fn into_stimulus(self) -> Channel<T> {
        let value = T::from_u8(self).unwrap();
        let max = T::from_u8(u8::MAX).unwrap();
        let reciprocal = T::one() / max;

        if max * reciprocal == T::one() {
            Channel(value * reciprocal)
        } else {
            Channel(value / max)
        }
    }
}
