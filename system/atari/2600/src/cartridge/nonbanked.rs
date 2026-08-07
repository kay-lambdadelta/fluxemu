use std::ops::RangeInclusive;

use bytes::Bytes;
use fluxemu_range::ContiguousRange;
use fluxemu_runtime::{
    Platform,
    component::{Component, config::ComponentConfig},
    machine::builder::ComponentBuilder,
    memory::{AddressSpaceId, MapTarget, MemoryMapCommand, Permissions},
};

use crate::cartridge::{CartType, get_cart_range};

#[derive(Debug)]
pub struct NonbankedCart;

impl Component for NonbankedCart {
    type Event = ();
}

#[derive(Debug)]
pub struct NonbankedCartConfig {
    pub rom: Bytes,
    pub cpu_address_space: AddressSpaceId,
    pub cart_type: CartType,
}

impl<P: Platform> ComponentConfig<P> for NonbankedCartConfig {
    type Component = NonbankedCart;

    fn build_component(
        self,
        component_builder: ComponentBuilder<P, Self::Component>,
    ) -> Result<Self::Component, Box<dyn std::error::Error>> {
        let cart_range = get_cart_range();

        match self.cart_type {
            CartType::Raw2k => {
                let halfway = cart_range.len() / 2;

                let low = RangeInclusive::from_start_and_length(*cart_range.start(), halfway);
                let high =
                    RangeInclusive::from_start_and_length(*cart_range.start() + halfway, halfway);

                let mappings = [
                    MemoryMapCommand::immutable_memory(*low.start(), self.rom),
                    MemoryMapCommand::Map {
                        range: high,
                        permissions: Permissions::READ,
                        target: MapTarget::Mirror { destination: low },
                    },
                ];

                component_builder.map_memory(self.cpu_address_space, mappings);
            }
            CartType::Raw4k => {
                component_builder.map_memory(
                    self.cpu_address_space,
                    [MemoryMapCommand::immutable_memory(
                        *cart_range.start(),
                        self.rom,
                    )],
                );
            }
            CartType::F8 | CartType::F6 => {
                unreachable!()
            }
        }

        Ok(NonbankedCart)
    }
}
