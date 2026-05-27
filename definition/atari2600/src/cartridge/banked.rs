use std::ops::RangeInclusive;

use bytes::Bytes;
use fluxemu_range::ContiguousRange;
use fluxemu_runtime::{
    component::{Component, config::ComponentConfig},
    machine::builder::ComponentBuilder,
    memory::{Address, AddressSpaceId, MemoryError, MemoryErrorType},
    platform::Platform,
};

use crate::cartridge::{CartType, get_cart_range};

impl CartType {
    fn find_matching_hotspot(self, address: Address) -> Option<usize> {
        let hotspots = match self {
            CartType::F8 => Some(RangeInclusive::from_start_and_length(0x1ff8, 2)),
            CartType::F6 => Some(RangeInclusive::from_start_and_length(0x1ff6, 4)),
            _ => None,
        }?;

        hotspots
            .contains(&address)
            .then(|| address - hotspots.start())
    }

    fn expected_rom_size(self) -> usize {
        self.bank_count() * get_cart_range().len()
    }
}

#[derive(Debug)]
pub struct BankedCart {
    rom: Bytes,
    cart_type: CartType,
    current_bank: usize,
}

impl Component for BankedCart {
    type Event = ();

    fn memory_read(
        &mut self,
        address: Address,
        _address_space: AddressSpaceId,
        avoid_side_effects: bool,
        buffer: &mut [u8],
    ) -> Result<(), MemoryError> {
        if !avoid_side_effects && let Some(bank) = self.cart_type.find_matching_hotspot(address) {
            self.current_bank = bank;
        }

        let cart_range = get_cart_range();

        let offset = address - cart_range.start();
        let source = self.rom.slice(RangeInclusive::from_start_and_length(
            self.current_bank * cart_range.len() + offset,
            buffer.len(),
        ));
        buffer.copy_from_slice(&source);

        Ok(())
    }

    fn memory_write(
        &mut self,
        address: Address,
        _address_space: AddressSpaceId,
        _buffer: &[u8],
    ) -> Result<(), MemoryError> {
        if let Some(bank) = self.cart_type.find_matching_hotspot(address) {
            self.current_bank = bank;
        }

        // In reality there is nothing here
        Err(MemoryError(
            std::iter::once((
                RangeInclusive::from_start_and_length(address, 1),
                MemoryErrorType::Denied,
            ))
            .collect(),
        ))
    }
}

#[derive(Debug)]
pub struct BankedCartConfig {
    pub rom: Bytes,
    pub cpu_address_space: AddressSpaceId,
    pub cart_type: CartType,
}

impl<P: Platform> ComponentConfig<P> for BankedCartConfig {
    type Component = BankedCart;

    fn build_component(
        self,
        component_builder: ComponentBuilder<P, Self::Component>,
    ) -> Result<Self::Component, Box<dyn std::error::Error>> {
        if self.rom.len() != self.cart_type.expected_rom_size() {
            return Err(format!(
                "ROM size {} does not match expected {} bytes for mapper {:?}",
                self.rom.len(),
                self.cart_type.expected_rom_size(),
                self.cart_type,
            )
            .into());
        }

        component_builder.memory_map_component_read(self.cpu_address_space, get_cart_range());

        Ok(BankedCart {
            rom: self.rom,
            cart_type: self.cart_type,
            current_bank: 0,
        })
    }
}
