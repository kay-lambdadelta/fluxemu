use std::ops::Range;

use fluxemu_runtime::{graphics::Ratio, scheduler::Frequency};

use super::Region;
use crate::tia::color::TiaColor;

#[derive(Debug)]
pub struct Pal;

impl Region for Pal {
    const DISPLAYED_SCANLINES: Range<u16> = 25..299;
    const TOTAL_SCANLINES: u16 = 312;

    fn frequency() -> Frequency {
        Frequency::from_num(17734475) / 4
    }

    fn pixel_aspect_ratio() -> Ratio<u32> {
        Ratio::new(1475000, 709379)
    }

    fn color_to_srgb(_color: TiaColor) -> palette::Srgb<u8> {
        todo!()
    }
}
