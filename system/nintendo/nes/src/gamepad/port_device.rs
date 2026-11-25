pub const DATA_LINE_MASK: u8 = 0b0001_1001;

pub trait PortDevice {
    fn set_strobe(&mut self, strobe: bool);
    fn read(&mut self, avoid_side_effects: bool) -> u8;
}
