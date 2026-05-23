use nalgebra::SVector;
use num::Float;

use crate::SampleFormat;

mod cubic;
mod linear;
mod nearest;

pub use cubic::Cubic;
pub use linear::Linear;
pub use nearest::Nearest;

/// Trait for interpolators, generic over frame size and sample format
pub trait Interpolator<S: SampleFormat, const CHANNELS: usize, INTERMEDIATE: Float + SampleFormat>:
    'static
{
    /// Interpolates a sequence of samples from a source rate to a target rate given an interpolator
    fn interpolate(
        &mut self,
        input: impl IntoIterator<Item = SVector<S, CHANNELS>>,
    ) -> impl Iterator<Item = SVector<S, CHANNELS>>;
}
