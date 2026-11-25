use alloc::boxed::Box;
use core::ops::RangeInclusive;

use fluxemu_math::range::ContiguousRange;
use fluxemu_platform::Platform;
use fluxemu_runtime::{
    RuntimeHandle,
    component::{Component, ComponentHandle, config::ComponentConfig},
    machine::builder::ComponentBuilder,
    memory::{Address, AddressSpaceId, MemoryError, MemoryMapCommand, Permissions},
};

use crate::gamepad::port_device::{DATA_LINE_MASK, PortDevice};

const PORT_0: Address = 0x4016;

#[derive(Debug)]
pub struct ControllerPorts {
    ports: [Option<ComponentHandle<dyn PortDevice>>; 2],
}

impl Component for ControllerPorts {
    type Event = ();
    type Save = ();
    type Snapshot = ();

    fn memory_read(
        &mut self,
        address: Address,
        _address_space: AddressSpaceId,
        avoid_side_effects: bool,
        buffer: &mut [u8],
    ) -> Result<(), MemoryError> {
        let data = match &self.ports[address - PORT_0] {
            Some(device) => RuntimeHandle::with_current(|runtime| {
                runtime
                    .component_registry()
                    .interact_handle(device, runtime.current_timestamp(), |device| {
                        device.read(avoid_side_effects)
                    })
                    .unwrap()
                    & DATA_LINE_MASK
            }),
            None => 0,
        };

        buffer[0] = ((address >> 8) as u8 & 0b1110_0000) | data;

        Ok(())
    }

    fn memory_write(
        &mut self,
        _address: Address,
        _address_space: AddressSpaceId,
        buffer: &[u8],
    ) -> Result<(), MemoryError> {
        let strobe = buffer[0] & 0b0000_0001 != 0;

        RuntimeHandle::with_current(|runtime| {
            let timestamp = runtime.current_timestamp();

            for device in self.ports.iter().flatten() {
                runtime
                    .component_registry()
                    .interact_handle(device, timestamp, |device| device.set_strobe(strobe))
                    .unwrap();
            }
        });

        Ok(())
    }
}

impl<P: Platform> ComponentConfig<P> for ControllerPortsConfig {
    type Component = ControllerPorts;

    fn build_component(
        self,
        component_builder: ComponentBuilder<P, Self::Component>,
    ) -> Result<Self::Component, Box<dyn core::error::Error>> {
        let my_path = component_builder.path().clone();
        component_builder.map_memory(
            self.cpu_address_space,
            MemoryMapCommand::with_component(
                my_path,
                [
                    (
                        RangeInclusive::from_start_and_length(PORT_0, 2),
                        Permissions::READ,
                    ),
                    (RangeInclusive::from_single(PORT_0), Permissions::WRITE),
                ],
            ),
        );

        Ok(ControllerPorts { ports: self.ports })
    }
}

#[derive(Debug)]
pub struct ControllerPortsConfig {
    pub cpu_address_space: AddressSpaceId,
    pub ports: [Option<ComponentHandle<dyn PortDevice>>; 2],
}
