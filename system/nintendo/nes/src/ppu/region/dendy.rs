use core::ops::Range;

use fluxemu_runtime::{graphics::Ratio, scheduler::Frequency};
use palette::Srgb;

use super::Region;

#[derive(Debug)]
pub struct Dendy;

impl Region for Dendy {
    const BYPASS_READ_BUFFER_FOR_PPUDATA_PALETTE_READS: bool = true;
    const CPU_CLOCK_DIVISOR: u8 = todo!();
    const DISPLAYED_SCANLINES: Range<u16> = todo!();
    const PPU_CLOCK_DIVISOR: u8 = todo!();
    const SKIPS_DOT_ON_ODD_FRAME: bool = true;
    const VBLANK_LENGTH: u16 = 0;
    const VISIBLE_SCANLINES: u16 = 0;

    fn master_clock() -> Frequency {
        todo!()
    }

    fn pixel_aspect_ratio() -> Ratio<u32> {
        todo!()
    }

    fn generate_palette() -> [[Srgb<u8>; 64]; 8] {
        todo!()
    }
}
