use std::{fmt::Debug, ops::Range};

use fluxemu_runtime::{graphics::Ratio, scheduler::Frequency};
use palette::Srgb;

use super::color::TiaColor;

pub mod ntsc;
pub mod pal;
pub mod secam;

pub trait Region: Send + Sync + Debug + 'static {
    const TOTAL_SCANLINES: u16;
    const DISPLAYED_SCANLINES: Range<u16>;

    fn frequency() -> Frequency;

    fn pixel_aspect_ratio() -> Ratio<u32>;

    fn color_to_srgb(color: TiaColor) -> Srgb<u8>;
}
