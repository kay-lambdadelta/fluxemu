use std::f32::consts::TAU;

use fluxemu_math::color::YUV_TO_RGB_SDTV_WITH_BT470;
use fluxemu_runtime::scheduler::Frequency;
use nalgebra::Rotation;
use palette::Srgb;

use crate::ppu::region::composite::{CompositeParams, build_palette};

use super::Region;

#[derive(Debug)]
pub struct Pal;

impl Region for Pal {
    const BYPASS_READ_BUFFER_FOR_PPUDATA_PALETTE_READS: bool = true;
    const VBLANK_LENGTH: u16 = 70;
    const VISIBLE_SCANLINES: u16 = 240;
    const SKIPS_DOT_ON_ODD_FRAME: bool = false;
    const PPU_CLOCK_DIVISOR: u8 = 5;

    fn master_clock() -> Frequency {
        // ~53.203425 MHZ / 2
        Frequency::from_num(53203425) / 2
    }

    fn generate_palette() -> [Srgb<u8>; 64] {
        build_palette(CompositeParams {
            black_voltage: 0.220,
            luma_voltage_grey: [0.160, 0.220, 0.480, 0.870],
            luma_voltage_chroma: [0.350, 0.530, 0.840, 1.035],
            chroma_amplitude: [0.190, 0.310, 0.360, 0.165],
            hue0_phase: Rotation::<_, 2>::new((-15.0f32).to_radians()),
            hue_step: Rotation::<_, 2>::new(TAU / 12.0),
            burst_phase: Rotation::<_, 2>::new((135.0f32).to_radians()),
            chroma_to_rgb: YUV_TO_RGB_SDTV_WITH_BT470,
        })
    }
}
