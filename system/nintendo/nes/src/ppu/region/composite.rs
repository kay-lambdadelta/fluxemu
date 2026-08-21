use std::f32::consts::TAU;

use nalgebra::{Rotation, SMatrix, SVector};
use palette::Srgb;

const SAMPLES_PER_PIXEL: usize = 8;
const WHITE_VOLTAGE: f32 = 1.1;

#[derive(Debug, Clone, Copy)]
pub struct CompositeParams {
    pub black_voltage: f32,
    pub luma_voltage_grey: [f32; 4],
    pub luma_voltage_chroma: [f32; 4],
    pub chroma_amplitude: [f32; 4],
    pub hue0_phase: Rotation<f32, 2>,
    pub hue_step: Rotation<f32, 2>,
    pub burst_phase: Rotation<f32, 2>,
    pub chroma_to_rgb: SMatrix<f32, 3, 2>,
}

impl CompositeParams {
    #[inline]
    fn sample_voltage(&self, hue: u8, level: u8, sample_index: usize) -> f32 {
        let level = level as usize;

        match hue {
            0 => self.luma_voltage_chroma[level] + self.chroma_amplitude[level],
            0xd => self.luma_voltage_grey[level],
            0xe | 0xf => self.luma_voltage_grey[1],
            1..=0xc => {
                let sample_phase =
                    Rotation::<_, 2>::new(TAU * sample_index as f32 / SAMPLES_PER_PIXEL as f32);

                let hue_phase = self.hue0_phase * self.hue_step.powf(f32::from(hue) - 1.0);
                let phase = sample_phase * hue_phase;

                let chroma_vector = phase * SVector::<_, 2>::new(self.chroma_amplitude[level], 0.0);

                self.luma_voltage_chroma[level] + chroma_vector.x
            }
            _ => unreachable!(),
        }
    }

    #[inline]
    fn decode(&self, waveform: &SMatrix<f32, SAMPLES_PER_PIXEL, 1>) -> SVector<f32, 3> {
        let reference = self.reference_basis();
        let voltage_range = WHITE_VOLTAGE - self.black_voltage;

        let yiq = reference * waveform;
        let normalized_y = (yiq.x - self.black_voltage) / voltage_range;

        let normalized_chroma = yiq.yz() / voltage_range;
        let rgb_chroma = self.chroma_to_rgb * normalized_chroma;

        SVector::from_element(normalized_y) + rgb_chroma
    }

    #[inline]
    fn reference_basis(&self) -> SMatrix<f32, 3, SAMPLES_PER_PIXEL> {
        SMatrix::from_fn(|row, sample| {
            let sample_phase = TAU * sample as f32 / SAMPLES_PER_PIXEL as f32;
            let phase = self.burst_phase * Rotation::<_, 2>::new(sample_phase);

            match row {
                0 => 1.0 / SAMPLES_PER_PIXEL as f32,
                1 => phase.angle().sin() * 2.0 / SAMPLES_PER_PIXEL as f32,
                2 => phase.angle().cos() * 2.0 / SAMPLES_PER_PIXEL as f32,
                _ => unreachable!(),
            }
        })
    }

    #[inline]
    fn decode_color<O: From<Srgb<f32>>>(&self, hue: u8, level: u8) -> O {
        let waveform = SMatrix::from_fn(|sample, _| self.sample_voltage(hue, level, sample));

        let color = self.decode(&waveform);
        let color = Srgb::new(color.x, color.y, color.z);

        color.into()
    }
}

pub fn build_palette(parameters: CompositeParams) -> [Srgb<u8>; 64] {
    std::array::from_fn(|index| {
        let hue = (index % 16) as u8;
        let level = ((index >> 4) % 4) as u8;

        parameters.decode_color(hue, level)
    })
}
