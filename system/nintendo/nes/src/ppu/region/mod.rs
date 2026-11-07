use core::{fmt::Debug, ops::Range};

use fluxemu_runtime::{graphics::Ratio, scheduler::Frequency};
use palette::Srgb;

use crate::ppu::DUMMY_SCANLINE_COUNT;

pub mod dendy;
pub mod ntsc;
pub mod pal;

pub mod composite;

pub trait Region: Send + Sync + Debug + 'static {
    const VISIBLE_SCANLINES: u16;
    const VBLANK_LENGTH: u16;
    const TOTAL_SCANLINES: u16 =
        Self::VISIBLE_SCANLINES + Self::VBLANK_LENGTH + DUMMY_SCANLINE_COUNT;
    const BYPASS_READ_BUFFER_FOR_PPUDATA_PALETTE_READS: bool;
    const PRERENDER_SCANLINE: u16 = Self::TOTAL_SCANLINES - 1;
    const SKIPS_DOT_ON_ODD_FRAME: bool;
    const PPU_CLOCK_DIVISOR: u8;
    const CPU_CLOCK_DIVISOR: u8;
    const DISPLAYED_SCANLINES: Range<u16>;

    fn master_clock() -> Frequency;
    fn pixel_aspect_ratio() -> Ratio<u32>;
    fn generate_palette() -> [[Srgb<u8>; 64]; 8];
}
