use std::{
    fmt::Debug,
    ops::RangeInclusive,
    sync::{
        Arc, Weak,
        atomic::{AtomicU8, Ordering},
    },
};

use fluxemu_definition_memory::{InitialContents, MemoryConfig};
use fluxemu_range::ContiguousRange;
use fluxemu_runtime::{
    component::{Component, ComponentConfig, LateContext, LateInitializedData},
    machine::{
        Machine,
        builder::{ComponentBuilder, SchedulerParticipation},
    },
    memory::{
        Address, AddressSpaceId, MapTarget, MemoryError, MemoryErrorType, MemoryRemappingCommand,
        Permissions,
    },
    path::ComponentPath,
    platform::Platform,
    scheduler::{Frequency, Period, SynchronizationContext},
};
use rangemap::RangeInclusiveMap;
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

#[derive(Serialize, Deserialize, Debug, Default, Clone, Copy)]
struct TimerConfiguration {
    timer: u8,
    divider: u16,
    next_timestamp: Period,
}

#[derive(Debug)]
pub struct Mos6532Riot {
    swacnt: bool,
    swbcnt: bool,
    instat: AtomicU8,
    timer_configuration: Option<TimerConfiguration>,
    timestamp: Period,
    machine: Weak<Machine>,
    config: Mos6532RiotConfig,
}

impl Mos6532Riot {
    pub fn swcha_address(&self) -> Address {
        self.config.registers_assigned_address + (Register::Swcha as Address)
    }

    pub fn swchb_address(&self) -> Address {
        self.config.registers_assigned_address + (Register::Swchb as Address)
    }
}

impl Component for Mos6532Riot {
    fn memory_read(
        &self,
        address: Address,
        _address_space: AddressSpaceId,
        _avoid_side_effects: bool,
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
                    *buffer_section = self.swacnt.into();
                }
                Register::Swchb => {
                    unreachable!()
                }
                Register::Swbcnt => {
                    *buffer_section = self.swbcnt.into();
                }
                Register::Intim => {
                    *buffer_section = self.timer_configuration.map(|t| t.timer).unwrap_or(0);
                    self.instat.fetch_and(0b0111_1111, Ordering::AcqRel);
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
        for (address, buffer_section) in
            RangeInclusive::from_start_and_length(address, buffer.len()).zip(buffer.iter())
        {
            let adjusted_address = address - self.config.registers_assigned_address;

            match Register::from_repr(adjusted_address).unwrap() {
                Register::Swcha => {
                    unreachable!()
                }
                Register::Swacnt => {
                    self.swacnt = *buffer_section != 0;

                    if let Some(swacnt) = &self.config.swcha {
                        let machine = self.machine.upgrade().unwrap();
                        let address = self.swcha_address();

                        let permissions = if self.swbcnt {
                            Permissions {
                                read: true,
                                write: false,
                            }
                        } else {
                            Permissions {
                                read: false,
                                write: true,
                            }
                        };

                        machine.remap_address_space(
                            self.config.assigned_address_space,
                            [MemoryRemappingCommand::Map {
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
                    self.swbcnt = *buffer_section != 0;

                    if let Some(swbcnt) = &self.config.swchb {
                        let machine = self.machine.upgrade().unwrap();
                        let address = self.swchb_address();

                        let permissions = if self.swbcnt {
                            Permissions {
                                read: true,
                                write: false,
                            }
                        } else {
                            Permissions {
                                read: false,
                                write: true,
                            }
                        };

                        machine.remap_address_space(
                            self.config.assigned_address_space,
                            [MemoryRemappingCommand::Map {
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
                Register::Tim1t => {
                    self.timer_configuration = Some(TimerConfiguration {
                        timer: *buffer_section,
                        divider: 1,
                        next_timestamp: self.timestamp + self.config.frequency.recip(),
                    });
                }
                Register::Tim8t => {
                    self.timer_configuration = Some(TimerConfiguration {
                        timer: *buffer_section,
                        divider: 8,
                        next_timestamp: self.timestamp + self.config.frequency.recip() * 8,
                    });
                }
                Register::Tim64t => {
                    self.timer_configuration = Some(TimerConfiguration {
                        timer: *buffer_section,
                        divider: 64,
                        next_timestamp: self.timestamp + self.config.frequency.recip() * 64,
                    });
                }
                Register::T1024t => {
                    self.timer_configuration = Some(TimerConfiguration {
                        timer: *buffer_section,
                        divider: 1024,
                        next_timestamp: self.timestamp + self.config.frequency.recip() * 1024,
                    });
                }
                Register::Instat => todo!(),
            }
        }

        Ok(())
    }

    fn synchronize(&mut self, mut context: SynchronizationContext) {
        for timestamp in context.allocate(self.config.frequency.recip(), None) {
            self.timestamp = timestamp;

            if let Some(config) = &mut self.timer_configuration
                && self.timestamp == config.next_timestamp
            {
                let (new_timer, underflowed) = config.timer.overflowing_sub(1);

                if !underflowed {
                    config.next_timestamp += self.config.frequency.recip() * config.divider as u128;
                } else {
                    config.divider = 1;
                    config.next_timestamp += self.config.frequency.recip();
                    self.instat.fetch_or(0b1000_0000, Ordering::AcqRel);
                }

                config.timer = new_timer;
            }
        }
    }

    fn needs_work(&self, delta: Period) -> bool {
        if let Some(config) = self.timer_configuration {
            self.timestamp + delta >= config.next_timestamp
        } else {
            false
        }
    }
}

impl<P: Platform> ComponentConfig<P> for Mos6532RiotConfig {
    type Component = Mos6532Riot;

    fn late_initialize(
        component: &mut Self::Component,
        data: &LateContext<P>,
    ) -> LateInitializedData<P> {
        component.machine = Arc::downgrade(&data.machine);

        LateInitializedData::default()
    }

    fn build_component(
        self,
        component_builder: ComponentBuilder<'_, '_, P, Self::Component>,
    ) -> Result<Self::Component, Box<dyn std::error::Error>> {
        let ram_assigned_addresses =
            self.ram_assigned_address..=self.ram_assigned_address.checked_add(0x7f).unwrap();

        let swacnt = (Register::Swacnt as Address) + self.registers_assigned_address;
        let swbcnt = (Register::Swbcnt as Address) + self.registers_assigned_address;
        let intim = (Register::Intim as Address) + self.registers_assigned_address;
        let tim1t = (Register::Tim1t as Address) + self.registers_assigned_address;
        let tim8t = (Register::Tim8t as Address) + self.registers_assigned_address;
        let tim64t = (Register::Tim64t as Address) + self.registers_assigned_address;
        let t1024t = (Register::T1024t as Address) + self.registers_assigned_address;
        let instat = (Register::Instat as Address) + self.registers_assigned_address;

        let component_builder = component_builder
            .memory_map_component(self.assigned_address_space, swacnt..=swacnt)
            .memory_map_component(self.assigned_address_space, swbcnt..=swbcnt)
            .memory_map_component_read(self.assigned_address_space, intim..=intim)
            .memory_map_component_write(self.assigned_address_space, tim1t..=tim1t)
            .memory_map_component_write(self.assigned_address_space, tim8t..=tim8t)
            .memory_map_component_write(self.assigned_address_space, tim64t..=tim64t)
            .memory_map_component_write(self.assigned_address_space, t1024t..=t1024t)
            .memory_map_component_read(self.assigned_address_space, instat..=instat)
            .scheduler_participation(SchedulerParticipation::OnAccess);

        component_builder.component(
            "ram",
            MemoryConfig {
                readable: true,
                writable: true,
                assigned_range: ram_assigned_addresses.clone(),
                assigned_address_space: self.assigned_address_space,
                initial_contents: RangeInclusiveMap::from_iter([(
                    ram_assigned_addresses,
                    InitialContents::Random,
                )]),
                sram: false,
            },
        );

        Ok(Self::Component {
            swacnt: false,
            swbcnt: false,
            instat: AtomicU8::new(0),
            config: self,
            timer_configuration: None,
            timestamp: Period::default(),
            machine: Weak::default(),
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
