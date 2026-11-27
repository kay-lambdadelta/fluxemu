use num::traits::{FromBytes, ToBytes, ops::bytes::NumBytes};
use serde::{Deserialize, Serialize};
use serde_with::{Bytes, serde_as};

use crate::memory::Address;

#[serde_as]
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct PrivateRam<const SIZE: usize, const WRAP: bool = true>(
    #[serde_as(as = "Bytes")] [u8; SIZE],
);

impl<const SIZE: usize, const WRAP: bool> PrivateRam<SIZE, WRAP> {
    const NOT_EMPTY: () = assert!(SIZE != 0, "Private RAM must not be empty");

    #[inline]
    pub const fn new(contents: [u8; SIZE]) -> Self {
        let () = Self::NOT_EMPTY;

        Self(contents)
    }

    #[inline]
    fn check_bounds(address: Address, length: usize) {
        assert!(
            address.checked_add(length).is_some_and(|end| end <= SIZE),
            "Access of {length} bytes at {address:#x} is outside private RAM of size {SIZE:#x}"
        );
    }

    #[inline]
    pub fn read<B: NumBytes + ?Sized>(&self, address: Address, buffer: &mut B) {
        let mut buffer = buffer.as_mut();

        if !WRAP {
            Self::check_bounds(address, buffer.len());
            buffer.copy_from_slice(&self.0[address..address + buffer.len()]);
            return;
        }

        let mut address = address % SIZE;

        while !buffer.is_empty() {
            let length = buffer.len().min(SIZE - address);
            let (chunk, remaining) = buffer.split_at_mut(length);

            chunk.copy_from_slice(&self.0[address..address + length]);

            buffer = remaining;
            address = 0;
        }
    }

    #[inline]
    pub fn read_le_value<T: FromBytes>(&self, address: Address) -> T
    where
        T::Bytes: Default,
    {
        let mut buffer = T::Bytes::default();
        self.read(address, &mut buffer);
        T::from_le_bytes(&buffer)
    }

    #[inline]
    pub fn read_be_value<T: FromBytes>(&self, address: Address) -> T
    where
        T::Bytes: Default,
    {
        let mut buffer = T::Bytes::default();
        self.read(address, &mut buffer);
        T::from_be_bytes(&buffer)
    }

    #[inline]
    pub fn write<B: NumBytes + ?Sized>(&mut self, address: Address, buffer: &B) {
        let mut buffer = buffer.as_ref();

        if !WRAP {
            Self::check_bounds(address, buffer.len());
            self.0[address..address + buffer.len()].copy_from_slice(buffer);
            return;
        }

        let mut address = address % SIZE;

        while !buffer.is_empty() {
            let length = buffer.len().min(SIZE - address);
            let (chunk, remaining) = buffer.split_at(length);

            self.0[address..address + length].copy_from_slice(chunk);

            buffer = remaining;
            address = 0;
        }
    }

    #[inline]
    pub fn write_le_value<T: ToBytes>(&mut self, address: Address, value: T) {
        self.write(address, &value.to_le_bytes());
    }

    #[inline]
    pub fn write_be_value<T: ToBytes>(&mut self, address: Address, value: T) {
        self.write(address, &value.to_be_bytes());
    }
}

impl<const SIZE: usize, const WRAP: bool> Default for PrivateRam<SIZE, WRAP> {
    #[inline]
    fn default() -> Self {
        Self::new([0; SIZE])
    }
}
