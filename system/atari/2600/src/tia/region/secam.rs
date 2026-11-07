use std::ops::Range;

use fluxemu_runtime::{graphics::Ratio, scheduler::Frequency};

use super::Region;
use crate::tia::color::TiaColor;

#[derive(Debug)]
pub struct Secam;

impl Region for Secam {
    const DISPLAYED_SCANLINES: Range<u16> = 25..299;
    const TOTAL_SCANLINES: u16 = 312;

    fn frequency() -> Frequency {
        todo!()
    }

    fn pixel_aspect_ratio() -> Ratio<u32> {
        todo!()
    }

    fn color_to_srgb(_color: TiaColor) -> palette::Srgb<u8> {
        todo!()
    }
}
