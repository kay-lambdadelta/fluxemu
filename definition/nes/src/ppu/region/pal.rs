use fluxemu_runtime::scheduler::Frequency;
use palette::{Srgb, named::BLACK};

use super::Region;

#[derive(Debug)]
pub struct Pal;

impl Region for Pal {
    const COLOR_PALETTE: [Srgb<u8>; 64] = [BLACK; 64];
    const VBLANK_LENGTH: u16 = 0;
    const VISIBLE_SCANLINES: u16 = 0;
    const BYPASS_READ_BUFFER_FOR_PPUDATA_PALETTE_READS: bool = true;

    fn master_clock() -> Frequency {
        Frequency::from_num(17734475) / 4
    }
}
