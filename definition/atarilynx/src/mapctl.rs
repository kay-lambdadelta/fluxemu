use fluxemu_runtime::{
    RuntimeApi,
    component::{
        Component,
        config::{ComponentConfig, LateContext, LateInitializedData},
    },
    machine::builder::ComponentBuilder,
    memory::{
        Address, AddressSpaceId, MapTarget, MemoryError, MemoryRemappingCommand, Permissions,
    },
    path::ComponentPath,
    platform::Platform,
};
use serde::{Deserialize, Serialize};

use crate::{
    MAPCTL_ADDRESS, MIKEY_ADDRESSES, RESERVED_MEMORY_ADDRESS, SUZY_ADDRESSES, VECTOR_ADDRESSES,
};

#[derive(Debug)]
pub struct Mapctl {
    config: MapctlConfig,
    status: MapctlStatus,
    my_path: ComponentPath,
}

impl Component for Mapctl {
    type Event = ();

    fn memory_read(
        &mut self,
        _address: Address,
        _address_space: AddressSpaceId,
        _avoid_side_effects: bool,
        buffer: &mut [u8],
    ) -> Result<(), MemoryError> {
        buffer[0] = self.status.to_byte();

        Ok(())
    }

    fn memory_write(
        &mut self,
        _address: Address,
        _address_space: AddressSpaceId,
        buffer: &[u8],
    ) -> Result<(), MemoryError> {
        let runtime = RuntimeApi::current();

        self.status = MapctlStatus::from_byte(buffer[0]);

        let mut remapping_commands = Vec::default();

        remapping_commands.push(MemoryRemappingCommand::Map {
            range: 0x0000..=0xffff,
            target: MapTarget::Component(self.config.ram.clone()),
            permissions: Permissions::ALL,
        });

        if self.status.suzy {
            remapping_commands.push(MemoryRemappingCommand::Map {
                range: SUZY_ADDRESSES,
                target: MapTarget::Component(self.config.suzy.clone()),
                permissions: Permissions::ALL,
            });
        }

        if self.status.mikey {
            remapping_commands.push(MemoryRemappingCommand::Map {
                range: MIKEY_ADDRESSES,
                target: MapTarget::Component(self.config.mikey.clone()),
                permissions: Permissions::ALL,
            });
        }

        remapping_commands.push(MemoryRemappingCommand::Unmap {
            range: RESERVED_MEMORY_ADDRESS..=RESERVED_MEMORY_ADDRESS,
            permissions: Permissions::ALL,
        });

        if self.status.vector {
            remapping_commands.push(MemoryRemappingCommand::Map {
                range: VECTOR_ADDRESSES,
                target: MapTarget::Component(self.config.vector.clone()),
                permissions: Permissions::ALL,
            });
        }

        remapping_commands.push(MemoryRemappingCommand::Map {
            range: MAPCTL_ADDRESS..=MAPCTL_ADDRESS,
            target: MapTarget::Component(self.my_path.clone()),
            permissions: Permissions::ALL,
        });

        let current_timestamp = runtime.registry().current_timestamp(&self.my_path).unwrap();

        runtime
            .address_space(self.config.cpu_address_space)
            .unwrap()
            .remap(current_timestamp, remapping_commands);

        Ok(())
    }
}

#[derive(Debug, Clone)]
pub struct MapctlConfig {
    pub ram: ComponentPath,
    pub suzy: ComponentPath,
    pub mikey: ComponentPath,
    pub vector: ComponentPath,
    pub cpu_address_space: AddressSpaceId,
}

impl<P: Platform> ComponentConfig<P> for MapctlConfig {
    type Component = Mapctl;

    fn late_initialize(
        _component: &mut Self::Component,
        _data: &LateContext<P>,
    ) -> LateInitializedData<P> {
        LateInitializedData::default()
    }

    fn build_component(
        self,
        component_builder: ComponentBuilder<'_, '_, P, Self::Component>,
    ) -> Result<Self::Component, Box<dyn std::error::Error>> {
        let my_path = component_builder.path().clone();

        component_builder.memory_map_component(self.cpu_address_space, 0xfff9..=0xfff9);

        Ok(Mapctl {
            config: self,
            status: Default::default(),
            my_path,
        })
    }
}

#[derive(Copy, Clone, PartialEq, Debug, Serialize, Deserialize, Default)]
pub struct MapctlStatus {
    pub suzy: bool,
    pub mikey: bool,
    pub rom: bool,
    pub vector: bool,
    pub reserved: u8, // 3 bits used
    pub sequential_disable: bool,
}

impl MapctlStatus {
    pub fn from_byte(byte: u8) -> Self {
        Self {
            suzy: byte & 0b0000_0001 != 0,
            mikey: byte & 0b0000_0010 != 0,
            rom: byte & 0b0000_0100 != 0,
            vector: byte & 0b0000_1000 != 0,
            reserved: (byte & 0b0111_0000) >> 4,
            sequential_disable: byte & 0b1000_0000 != 0,
        }
    }

    pub fn to_byte(self) -> u8 {
        (self.suzy as u8)
            | (self.mikey as u8) << 1
            | (self.rom as u8) << 2
            | (self.vector as u8) << 3
            | (self.reserved & 0b0000_0111) << 4
            | (self.sequential_disable as u8) << 7
    }
}
