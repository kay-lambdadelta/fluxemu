use alloc::boxed::Box;
use core::ops::RangeInclusive;

use fluxemu_math::range::ContiguousRange;
use fluxemu_platform::Platform;
use fluxemu_runtime::{
    RuntimeHandle,
    component::{
        Component,
        config::{ComponentConfig, LateContext},
    },
    machine::builder::ComponentBuilder,
    memory::{Address, AddressSpaceId, MapTarget, MemoryError, MemoryMapCommand, Permissions},
    persistence::PersistentComponentState,
};
use serde::{Deserialize, Serialize};

use crate::cartridge::{CartParams, rom_bank};

const PRG_BANK_SIZE: usize = 16 * 1024;

#[derive(Debug, Serialize, Deserialize)]
pub struct State {
    prg_bank_index: usize,
}

#[derive(Debug)]
pub struct UxRom {
    state: State,
    config: UxRomConfig,
}

impl PersistentComponentState<UxRom> for State {
    fn capture(component: &UxRom) -> impl Serialize {
        &component.state
    }

    fn apply(self, component: &mut UxRom) {
        component.state = self;
    }
}

impl UxRom {
    fn prg_bank_count(&self) -> usize {
        self.config.config.prg_rom.len() / PRG_BANK_SIZE
    }

    fn switchable_bank_offset(&self) -> usize {
        (self.state.prg_bank_index % self.prg_bank_count()) * PRG_BANK_SIZE
    }

    fn update_prg_banking(&self, runtime: &RuntimeHandle) {
        runtime
            .address_space(self.config.config.cpu_address_space)
            .unwrap()
            .remap(
                runtime.current_timestamp(),
                [rom_bank(
                    &self.config.config.prg_rom,
                    0x8000,
                    PRG_BANK_SIZE,
                    self.state.prg_bank_index,
                )],
            );
    }

    fn rom_byte_at(&self, address: Address) -> u8 {
        let prg_rom = &self.config.config.prg_rom;

        match address {
            0x8000..=0xbfff => prg_rom[self.switchable_bank_offset() + (address - 0x8000)],
            0xc000..=0xffff => prg_rom[prg_rom.len() - PRG_BANK_SIZE + (address - 0xc000)],
            _ => unreachable!(),
        }
    }
}

impl Component for UxRom {
    type Event = ();
    type Save = ();
    type Snapshot = State;

    fn state_restored(&mut self) {
        RuntimeHandle::with_current(|runtime| self.update_prg_banking(runtime));
    }

    fn memory_write(
        &mut self,
        address: Address,
        _address_space: AddressSpaceId,
        buffer: &[u8],
    ) -> Result<(), MemoryError> {
        RuntimeHandle::with_current(|runtime| {
            for (address, byte) in RangeInclusive::from_start_and_length(address, buffer.len())
                .zip(buffer.iter().copied())
            {
                let byte = if self.config.bus_conflicts {
                    byte & self.rom_byte_at(address)
                } else {
                    byte
                };

                let prg_bank_index = usize::from(byte);

                if prg_bank_index != self.state.prg_bank_index {
                    self.state.prg_bank_index = prg_bank_index;
                    self.update_prg_banking(runtime);
                }
            }
        });

        Ok(())
    }
}

#[derive(Debug)]
pub struct UxRomConfig {
    pub config: CartParams,
    pub bus_conflicts: bool,
}

impl<P: Platform> ComponentConfig<P> for UxRomConfig {
    type Component = UxRom;

    fn late_initialize(component: &mut Self::Component, _data: &LateContext<P>) {
        RuntimeHandle::with_current(|runtime| component.update_prg_banking(runtime));
    }

    fn build_component(
        self,
        mut component_builder: ComponentBuilder<P, Self::Component>,
    ) -> Result<Self::Component, Box<dyn core::error::Error>> {
        let prg_rom_size = self.config.prg_rom.len();

        if prg_rom_size == 0 || !prg_rom_size.is_multiple_of(PRG_BANK_SIZE) {
            return Err("UxROM PRG-ROM must be a non-zero multiple of 16KB".into());
        }

        component_builder = component_builder.map_memory(
            self.config.cpu_address_space,
            [MemoryMapCommand::immutable_memory(
                0xc000,
                self.config.prg_rom.slice(prg_rom_size - PRG_BANK_SIZE..),
            )],
        );

        component_builder = self.config.map_fixed_chr(component_builder)?;

        let my_path = component_builder.path().clone();

        component_builder.map_memory(
            self.config.cpu_address_space,
            [MemoryMapCommand::Map {
                range: 0x8000..=0xffff,
                permissions: Permissions::WRITE,
                target: MapTarget::Component(my_path),
            }],
        );

        Ok(UxRom {
            state: State { prg_bank_index: 0 },
            config: self,
        })
    }
}
