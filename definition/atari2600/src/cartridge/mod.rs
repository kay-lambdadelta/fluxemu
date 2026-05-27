use std::ops::RangeInclusive;

use fluxemu_range::ContiguousRange;
use fluxemu_runtime::memory::Address;

pub mod banked;
pub mod nonbanked;

fn get_cart_range() -> RangeInclusive<Address> {
    RangeInclusive::from_start_and_length(0x1000, 0x1000)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CartType {
    Raw2k,
    Raw4k,
    F8,
    F6,
}

impl CartType {
    pub fn detect(rom: &[u8]) -> Self {
        match rom.len() {
            0x800 => CartType::Raw2k,
            0x1000 => CartType::Raw4k,
            0x2000 => CartType::F8,
            0x4000 => CartType::F6,
            _ => unreachable!(),
        }
    }

    fn bank_count(self) -> usize {
        match self {
            CartType::Raw2k | CartType::Raw4k => 1,
            CartType::F8 => 2,
            CartType::F6 => 4,
        }
    }
}
