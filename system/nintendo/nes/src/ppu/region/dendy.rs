use fluxemu_runtime::scheduler::Frequency;
use palette::Srgb;

use super::Region;

#[derive(Debug)]
pub struct Dendy;

impl Region for Dendy {
    const BYPASS_READ_BUFFER_FOR_PPUDATA_PALETTE_READS: bool = true;
    const VBLANK_LENGTH: u16 = 0;
    const VISIBLE_SCANLINES: u16 = 0;
    const SKIPS_DOT_ON_ODD_FRAME: bool = true;
    const PPU_CLOCK_DIVISOR: u8 = todo!();
    const CPU_CLOCK_DIVISOR: u8 = todo!();

    fn master_clock() -> Frequency {
        todo!()
    }

    fn generate_palette() -> [Srgb<u8>; 64] {
        todo!()
    }
}
