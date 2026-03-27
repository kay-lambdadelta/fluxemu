use std::fmt::Debug;

use fluxemu_runtime::scheduler::Frequency;
use palette::Srgb;

use crate::ppu::DUMMY_SCANLINE_COUNT;

pub mod dendy;
pub mod ntsc;
pub mod pal;

pub trait Region: Send + Sync + Debug + 'static {
    const VISIBLE_SCANLINES: u16;
    const VBLANK_LENGTH: u16;
    const TOTAL_SCANLINES: u16 =
        Self::VISIBLE_SCANLINES + Self::VBLANK_LENGTH + DUMMY_SCANLINE_COUNT;
    const COLOR_PALETTE: [Srgb<u8>; 64];

    fn master_clock() -> Frequency;
}
