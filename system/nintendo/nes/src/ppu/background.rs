use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug)]
pub struct BackgroundState {
    pub pattern_table_index: u8,
    pub rendering_enabled: bool,
    // Shift registers
    pub pattern_low_shift: u16,
    pub pattern_high_shift: u16,
    pub attribute_shift: u32,
    /// Usually called x, 3 bits
    pub fine_x_scroll: u8,
    pub show_leftmost_pixels: bool,
    pub fetched_nametable: u8,
    pub fetched_attribute: u8,
    pub fetched_pattern_table_low: u8,
}
