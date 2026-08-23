use std::{fmt::Debug, ops::RangeInclusive};

use fluxemu_math::range::ContiguousRange;
use fluxemu_runtime::{
    RuntimeHandle,
    component::{
        Component,
        config::{ComponentConfig, LateContext},
    },
    machine::builder::ComponentBuilder,
    memory::{
        Address, AddressSpaceId, MapTarget, MemoryError, MemoryErrorType, MemoryMapCommand,
        Permissions,
    },
    path::ComponentPath,
    platform::Platform,
    scheduler::{
        Frequency, Period,
        event::{Event, EventMode, downcast_event},
    },
};
use serde::{Deserialize, Serialize};
use strum::FromRepr;

#[repr(usize)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, FromRepr)]
pub enum Register {
    Swcha = 0x00,
    Swacnt = 0x01,
    Swchb = 0x02,
    Swbcnt = 0x03,
    Intim = 0x04,
    Instat = 0x05,
    Tim1t = 0x14,
    Tim8t = 0x15,
    Tim64t = 0x16,
    T1024t = 0x17,
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy)]
struct TimerSnapshot {
    value: u8,
    timestamp: Period,
    divider: u16,
}

#[derive(Serialize, Deserialize, Debug)]
struct State {
    swacnt: bool,
    swbcnt: bool,
    instat: u8,
    timer: Option<TimerSnapshot>,
}

#[derive(Debug, Clone)]
pub enum RiotEvent {
    TimerUnderflow,
}

#[derive(Debug)]
pub struct Mos6532Riot {
    state: State,
    config: Mos6532RiotConfig,
    path: ComponentPath,
    period: Period,
}

impl Mos6532Riot {
    pub fn swcha_address(&self) -> Address {
        self.config.registers_assigned_address + (Register::Swcha as Address)
    }

    pub fn swchb_address(&self) -> Address {
        self.config.registers_assigned_address + (Register::Swchb as Address)
    }

    #[inline]
    fn compute_intim(&self) -> u8 {
        let Some(TimerSnapshot {
            value,
            timestamp,
            divider,
        }) = self.state.timer
        else {
            return 0;
        };

        let elapsed_ticks = RuntimeHandle::with_current(|handle| {
            ((handle.current_timestamp() - timestamp) / self.period).to_num::<u64>()
                / divider as u64
        });
        value.wrapping_sub(elapsed_ticks as u8)
    }
}

impl Component for Mos6532Riot {
    type Event = RiotEvent;

    fn memory_read(
        &mut self,
        address: Address,
        _address_space: AddressSpaceId,
        avoid_side_effects: bool,
        buffer: &mut [u8],
    ) -> Result<(), MemoryError> {
        for (address, buffer_section) in
            RangeInclusive::from_start_and_length(address, buffer.len()).zip(buffer.iter_mut())
        {
            let adjusted_address = address - self.config.registers_assigned_address;

            match Register::from_repr(adjusted_address).unwrap() {
                Register::Swcha => {
                    unreachable!()
                }
                Register::Swacnt => {
                    *buffer_section = self.state.swacnt.into();
                }
                Register::Swchb => {
                    unreachable!()
                }
                Register::Swbcnt => {
                    *buffer_section = self.state.swbcnt.into();
                }
                Register::Intim => {
                    *buffer_section = self.compute_intim();

                    if !avoid_side_effects {
                        self.state.instat &= 0b0111_1111;
                    }
                }
                Register::Instat => todo!(),
                _ => {
                    return Err(MemoryError(
                        std::iter::once((
                            RangeInclusive::from_start_and_length(address, buffer.len()),
                            MemoryErrorType::Denied,
                        ))
                        .collect(),
                    ));
                }
            }
        }

        Ok(())
    }

    fn memory_write(
        &mut self,
        address: Address,
        _address_space: AddressSpaceId,
        buffer: &[u8],
    ) -> Result<(), MemoryError> {
        RuntimeHandle::with_current(|runtime| {
            let timestamp = runtime.current_timestamp();

            for (address, buffer_section) in
                RangeInclusive::from_start_and_length(address, buffer.len()).zip(buffer.iter())
            {
                let adjusted_address = address - self.config.registers_assigned_address;
                let register = Register::from_repr(adjusted_address).unwrap();

                match register {
                    Register::Swcha => {
                        unreachable!()
                    }
                    Register::Swacnt => {
                        self.state.swacnt = *buffer_section != 0;

                        if let Some(swacnt) = &self.config.swcha {
                            let address = self.swcha_address();

                            let permissions = if self.state.swacnt {
                                Permissions::WRITE
                            } else {
                                Permissions::READ
                            };

                            runtime
                                .address_space(self.config.assigned_address_space)
                                .unwrap()
                                .remap(
                                    &timestamp,
                                    [MemoryMapCommand::Map {
                                        range: address..=address,
                                        target: MapTarget::Component(swacnt.clone()),
                                        permissions,
                                    }],
                                );
                        }
                    }
                    Register::Swchb => {
                        unreachable!()
                    }
                    Register::Swbcnt => {
                        self.state.swbcnt = *buffer_section != 0;

                        if let Some(swbcnt) = &self.config.swchb {
                            let address = self.swchb_address();

                            let permissions = if self.state.swbcnt {
                                Permissions::WRITE
                            } else {
                                Permissions::READ
                            };

                            runtime
                                .address_space(self.config.assigned_address_space)
                                .unwrap()
                                .remap(
                                    &timestamp,
                                    [MemoryMapCommand::Map {
                                        range: address..=address,
                                        target: MapTarget::Component(swbcnt.clone()),
                                        permissions,
                                    }],
                                );
                        }
                    }
                    Register::Intim => {
                        // Read only
                        unreachable!()
                    }
                    Register::Tim1t | Register::Tim8t | Register::Tim64t | Register::T1024t => {
                        let divider = match register {
                            Register::Tim1t => 1,
                            Register::Tim8t => 8,
                            Register::Tim64t => 64,
                            Register::T1024t => 1024,
                            _ => unreachable!(),
                        };

                        RuntimeHandle::with_current(|runtime| {
                            self.state.timer = Some(TimerSnapshot {
                                value: *buffer_section,
                                timestamp: runtime.current_timestamp(),
                                divider,
                            });

                            runtime.schedule_event_relative::<Self>(
                                &self.path,
                                EventMode::Once,
                                self.period * divider as u128 * (*buffer_section as u128 + 1),
                                RiotEvent::TimerUnderflow,
                            );
                        });
                    }
                    Register::Instat => todo!(),
                }
            }
        });

        Ok(())
    }

    fn handle_event(&mut self, event: Box<dyn Event>) {
        let event = downcast_event::<Self>(event);

        match event {
            RiotEvent::TimerUnderflow => {
                self.state.instat |= 0b1000_0000;

                RuntimeHandle::with_current(|runtime| {
                    self.state.timer = Some(TimerSnapshot {
                        value: 0xff,
                        timestamp: runtime.current_timestamp(),
                        divider: 1,
                    });
                });
            }
        }
    }
}

impl<P: Platform> ComponentConfig<P> for Mos6532RiotConfig {
    type Component = Mos6532Riot;

    fn late_initialize(component: &mut Self::Component, _data: &LateContext<P>) {
        let swcha_address =
            (Register::Swcha as Address) + component.config.registers_assigned_address;
        let swchb_address =
            (Register::Swchb as Address) + component.config.registers_assigned_address;

        RuntimeHandle::with_current(|runtime| {
            let mut mapping_commands = Vec::default();

            if let Some(swcha) = &component.config.swcha {
                mapping_commands.push(MemoryMapCommand::Map {
                    range: swcha_address..=swcha_address,
                    target: MapTarget::Component(swcha.clone()),
                    permissions: Permissions::READ,
                });
            }

            if let Some(swchb) = &component.config.swchb {
                mapping_commands.push(MemoryMapCommand::Map {
                    range: swchb_address..=swchb_address,
                    target: MapTarget::Component(swchb.clone()),
                    permissions: Permissions::READ,
                });
            }

            runtime
                .address_space(component.config.assigned_address_space)
                .unwrap()
                .remap(&Period::ZERO, mapping_commands);
        });
    }

    fn build_component(
        self,
        component_builder: ComponentBuilder<P, Self::Component>,
    ) -> Result<Self::Component, Box<dyn std::error::Error>> {
        let ram_assigned_addresses =
            RangeInclusive::from_start_and_length(self.ram_assigned_address, 0x80);

        let swacnt = (Register::Swacnt as Address) + self.registers_assigned_address;
        let swbcnt = (Register::Swbcnt as Address) + self.registers_assigned_address;
        let intim = (Register::Intim as Address) + self.registers_assigned_address;
        let tim1t = (Register::Tim1t as Address) + self.registers_assigned_address;
        let tim8t = (Register::Tim8t as Address) + self.registers_assigned_address;
        let tim64t = (Register::Tim64t as Address) + self.registers_assigned_address;
        let t1024t = (Register::T1024t as Address) + self.registers_assigned_address;
        let instat = (Register::Instat as Address) + self.registers_assigned_address;

        let my_path = component_builder.path().clone();

        let component_builder = component_builder.map_memory(
            self.assigned_address_space,
            MemoryMapCommand::with_component(
                my_path,
                [
                    (RangeInclusive::from_single(swacnt), Permissions::ALL),
                    (RangeInclusive::from_single(swbcnt), Permissions::ALL),
                    (RangeInclusive::from_single(intim), Permissions::READ),
                    (RangeInclusive::from_single(tim1t), Permissions::WRITE),
                    (RangeInclusive::from_single(tim8t), Permissions::WRITE),
                    (RangeInclusive::from_single(tim64t), Permissions::WRITE),
                    (RangeInclusive::from_single(t1024t), Permissions::WRITE),
                    (RangeInclusive::from_single(instat), Permissions::READ),
                ],
            ),
        );

        let path = component_builder.path().clone();

        let (component_builder, ram_path) =
            component_builder.memory("ram", ram_assigned_addresses.len(), []);

        component_builder.map_memory(
            self.assigned_address_space,
            [MemoryMapCommand::Map {
                range: ram_assigned_addresses,
                permissions: Permissions::ALL,
                target: MapTarget::Memory {
                    path: ram_path,
                    subrange: None,
                },
            }],
        );

        Ok(Self::Component {
            state: State {
                swacnt: false,
                swbcnt: false,
                instat: 0,
                timer: None,
            },
            period: self.frequency.recip(),
            config: self,
            path,
        })
    }
}

#[derive(Debug)]
pub struct Mos6532RiotConfig {
    pub frequency: Frequency,
    pub registers_assigned_address: Address,
    pub ram_assigned_address: Address,
    pub assigned_address_space: AddressSpaceId,
    pub swcha: Option<ComponentPath>,
    pub swchb: Option<ComponentPath>,
}
