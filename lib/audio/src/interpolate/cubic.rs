use core::marker::PhantomData;

use nalgebra::SVector;
use num::Float;
use ringbuffer::{ConstGenericRingBuffer, RingBuffer};

use super::Interpolator;
use crate::{FrameIterator, FromSample, SampleFormat};

/// Cubic interpolation
#[derive(Debug)]
pub struct Cubic<S: SampleFormat, const CHANNELS: usize, F: Float + SampleFormat = f32> {
    step: F,
    phase: F,
    held_samples: ConstGenericRingBuffer<SVector<F, CHANNELS>, 4>,
    source_rate: f32,
    target_rate: f32,
    _phantom: PhantomData<S>,
}

impl<S: SampleFormat, const CHANNELS: usize, F: Float + SampleFormat> Cubic<S, CHANNELS, F> {
    pub fn new(source_rate: f32, target_rate: f32) -> Self {
        let step = F::from_f32(source_rate / target_rate).unwrap();

        Self {
            step,
            phase: F::zero(),
            held_samples: ConstGenericRingBuffer::default(),
            source_rate,
            target_rate,
            _phantom: PhantomData,
        }
    }

    pub fn source_rate(&self) -> f32 {
        self.source_rate
    }

    pub fn target_rate(&self) -> f32 {
        self.target_rate
    }
}

impl<S: SampleFormat, const CHANNELS: usize, F: Float + SampleFormat> Interpolator<S, CHANNELS, F>
    for Cubic<S, CHANNELS, F>
where
    F: FromSample<S>,
    S: FromSample<F>,
{
    fn interpolate(
        &mut self,
        input: impl IntoIterator<Item = SVector<S, CHANNELS>>,
    ) -> impl Iterator<Item = SVector<S, CHANNELS>> {
        let mut input = input.into_iter().rescale::<F>();
        let mut input_exhausted = false;

        for _ in self.held_samples.len()..4 {
            if let Some(sample) = input.next() {
                self.held_samples.enqueue(sample);
            } else {
                self.held_samples
                    .enqueue(SVector::from_element(F::equilibrium()));
                input_exhausted = true;
            }
        }

        CubicIterator {
            state: self,
            input,
            input_exhausted,
        }
        .rescale::<S>()
    }
}

struct CubicIterator<
    'a,
    S: SampleFormat,
    const CHANNELS: usize,
    F: Float + SampleFormat,
    I: Iterator<Item = SVector<F, CHANNELS>>,
> {
    state: &'a mut Cubic<S, CHANNELS, F>,
    input: I,
    input_exhausted: bool,
}

impl<
    'a,
    S: SampleFormat,
    const CHANNELS: usize,
    F: Float + SampleFormat,
    I: Iterator<Item = SVector<F, CHANNELS>>,
> Iterator for CubicIterator<'a, S, CHANNELS, F, I>
{
    type Item = SVector<F, CHANNELS>;

    fn next(&mut self) -> Option<Self::Item> {
        while self.state.phase >= F::one() {
            if let Some(sample) = self.input.next() {
                self.state.held_samples.enqueue(sample);
                self.state.phase -= F::one();
            } else {
                self.input_exhausted = true;
                break;
            }
        }

        if self.input_exhausted && self.state.phase >= F::one() {
            return None;
        }

        let interpolated_sample = cubic_interpolate(
            &self.state.held_samples[0],
            &self.state.held_samples[1],
            &self.state.held_samples[2],
            &self.state.held_samples[3],
            self.state.phase,
        );

        self.state.phase += self.state.step;

        Some(interpolated_sample)
    }
}

#[inline]
fn cubic_interpolate<F: Float + SampleFormat, const CHANNELS: usize>(
    y0: &SVector<F, CHANNELS>,
    y1: &SVector<F, CHANNELS>,
    y2: &SVector<F, CHANNELS>,
    y3: &SVector<F, CHANNELS>,
    mu: F,
) -> SVector<F, CHANNELS> {
    let mu2 = mu.powi(2);
    let a0 = y3 - y2 - y0 + y1;
    let a1 = y0 - y1 - a0;
    let a2 = y2 - y0;
    let a3 = y1;

    a0 * mu * mu2 + a1 * mu2 + a2 * mu + a3
}
