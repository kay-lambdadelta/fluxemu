#[derive(Debug, Default)]
pub struct ShiftRegister {
    value: u8,
    count: u8,
}

impl ShiftRegister {
    pub fn shift(&mut self, bit: bool) -> Option<u8> {
        self.value |= (bit as u8) << self.count;
        self.count += 1;

        if self.count == 5 {
            let result = self.value;
            *self = Self::default();

            Some(result)
        } else {
            None
        }
    }
}
