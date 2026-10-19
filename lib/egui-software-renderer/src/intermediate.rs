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

#[inline]
pub fn from_usize<T: Intermediate>(value: usize) -> T {
    T::from_usize(value)
        .unwrap_or_else(|| panic!("value is out of range for the intermediate type: {}", value))
}

#[inline]
pub fn from_f32<T: Intermediate>(value: f32) -> T {
    T::from_f32(value)
        .unwrap_or_else(|| panic!("value is out of range for the intermediate type: {}", value))
}

#[inline]
pub fn half<T: Intermediate>() -> T {
    T::one() / (T::one() + T::one())
}

#[inline]
pub fn floor<T: Intermediate>(value: T) -> T {
    value - value.rem_euclid(&T::one())
}

#[inline]
pub fn ceil<T: Intermediate>(value: T) -> T {
    -floor(-value)
}

#[inline]
pub fn min<T: Intermediate>(a: T, b: T) -> T {
    if b < a { b } else { a }
}

#[inline]
pub fn max<T: Intermediate>(a: T, b: T) -> T {
    if b > a { b } else { a }
}

#[inline]
pub fn component_min<T: Intermediate>(a: Point2<T>, b: Point2<T>) -> Point2<T> {
    Point2::from(a.coords.zip_map(&b.coords, min))
}

#[inline]
pub fn component_max<T: Intermediate>(a: Point2<T>, b: Point2<T>) -> Point2<T> {
    Point2::from(a.coords.zip_map(&b.coords, max))
}

#[inline]
pub fn is_finite<T: Intermediate>(value: T) -> bool {
    (value * T::zero()).is_zero()
}

#[inline]
pub fn to_index<T: Intermediate>(value: T) -> usize {
    if value.partial_cmp(&T::zero()) != Some(Ordering::Greater) {
        return 0;
    }

    value.to_usize().unwrap_or(usize::MAX)
}

#[inline]
pub fn to_texel_index<T: Intermediate>(value: T, max: u32) -> u32 {
    if value.partial_cmp(&T::zero()) != Some(Ordering::Greater) {
        return 0;
    }

    value.to_u32().map_or(max, |value| value.min(max))
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
        Channel(max(self.0, min.0))
    }

    #[inline]
    fn clamp_max(self, max: Self) -> Self {
        Channel(min(self.0, max.0))
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
            * from_usize::<T>(u8::MAX as usize)
            + half::<T>();

        scaled.to_u8().unwrap_or(u8::MAX)
    }
}

impl<T: Intermediate> IntoStimulus<Channel<T>> for u8 {
    #[inline]
    fn into_stimulus(self) -> Channel<T> {
        Channel(from_usize::<T>(self as usize) / from_usize::<T>(u8::MAX as usize))
    }
}

#[inline]
pub fn max_int<T: Intermediate>() -> T {
    floor(T::max_value())
}
