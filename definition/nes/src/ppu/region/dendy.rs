use fluxemu_runtime::scheduler::Frequency;
use palette::{Srgb, named::BLACK};

use super::Region;

#[derive(Debug)]
pub struct Dendy;

impl Region for Dendy {
    const VBLANK_LENGTH: u16 = 0;
    const VISIBLE_SCANLINES: u16 = 0;
    const COLOR_PALETTE: [Srgb<u8>; 64] = [BLACK; 64];

    fn master_clock() -> Frequency {
        todo!()
    }
}
