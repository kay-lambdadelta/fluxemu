use std::f32::consts::TAU;

use fluxemu_math::color::YIQ_TO_RGB_NTSC_1953;
use fluxemu_runtime::scheduler::Frequency;
use nalgebra::{Rotation, SMatrix};
use palette::Srgb;

use crate::ppu::region::composite::{CompositeParams, build_palette};

use super::Region;

#[derive(Debug)]
pub struct Ntsc;

impl Region for Ntsc {
    const BYPASS_READ_BUFFER_FOR_PPUDATA_PALETTE_READS: bool = true;
    const VBLANK_LENGTH: u16 = 20;
    const VISIBLE_SCANLINES: u16 = 240;
    const SKIPS_DOT_ON_ODD_FRAME: bool = true;
    const PPU_CLOCK_DIVISOR: u8 = 4;
    const CPU_CLOCK_DIVISOR: u8 = 12;

    #[inline]
    fn master_clock() -> Frequency {
        // 236.25 MHz / 11
        Frequency::from_num(236250000) / 11
    }

    #[inline]
    fn generate_palette() -> [Srgb<u8>; 64] {
        // If something looks wrong, please consult and check against https://www.nesdev.org/wiki/NTSC_video
        //
        // Especially if you have a better understanding of math or televisions than I do.

        build_palette(CompositeParams {
            black_voltage: 0.312,
            luma_voltage_grey: [0.228, 0.312, 0.552, 0.880],
            luma_voltage_chroma: [0.422, 0.576, 0.826, 0.990],
            chroma_amplitude: [0.194, 0.264, 0.274, 0.110],
            hue0_phase: Rotation::<_, 2>::new(0.0),
            hue_step: Rotation::<_, 2>::new(TAU / 12.0),
            burst_phase: Rotation::<_, 2>::new((180.0f32).to_radians()),
            // Flip it so it works with the same math that works with pal
            chroma_to_rgb: SMatrix::from_fn(|row, col| YIQ_TO_RGB_NTSC_1953[(row, 1 - col)]),
        })
    }
}
