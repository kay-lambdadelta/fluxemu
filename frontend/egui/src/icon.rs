pub const FONT_NAME: &str = "lucide";
pub const FONT: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/lucide.ttf"));

include!(concat!(env!("OUT_DIR"), "/icon.rs"));
