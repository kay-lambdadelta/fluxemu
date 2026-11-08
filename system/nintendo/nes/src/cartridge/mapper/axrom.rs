use alloc::boxed::Box;
use core::ops::RangeInclusive;

use fluxemu_math::range::ContiguousRange;
use fluxemu_runtime::{
    RuntimeHandle,
    component::{
        Component,
        config::{ComponentConfig, LateContext},
    },
    machine::builder::ComponentBuilder,
    memory::{Address, AddressSpaceId, MapTarget, MemoryError, MemoryMapCommand, Permissions},
    persistence::PersistentComponentState,
    platform::Platform,
};
use serde::{Deserialize, Serialize};

use crate::cartridge::{CartParams, Mirroring, rom_bank};

const PRG_BANK_SIZE: usize = 32 * 1024;

#[derive(Debug, Serialize, Deserialize)]
pub struct State {
    prg_bank_index: usize,
    mirroring: Mirroring,
}

#[derive(Debug)]
pub struct AxRom {
    state: State,
    config: AxRomConfig,
}

impl PersistentComponentState<AxRom> for State {
    fn capture(component: &AxRom) -> impl Serialize {
        &component.state
    }

    fn apply(self, component: &mut AxRom) {
        component.state = self;
    }
}

impl AxRom {
    fn bank_offset(&self) -> usize {
        let prg_bank_count = self.config.config.prg_rom.len() / PRG_BANK_SIZE;

        (self.state.prg_bank_index % prg_bank_count) * PRG_BANK_SIZE
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

    fn update_nametables(&self, runtime: &RuntimeHandle) {
        runtime
            .address_space(self.config.config.ppu_address_space)
            .unwrap()
            .remap(
                runtime.current_timestamp(),
                self.state
                    .mirroring
                    .nametable_commands(&self.config.config.nametables),
            );
    }

    fn rom_byte_at(&self, address: Address) -> u8 {
        self.config.config.prg_rom[self.bank_offset() + (address - 0x8000)]
    }
}

impl Component for AxRom {
    type Event = ();
    type Save = ();
    type Snapshot = State;

    fn state_restored(&mut self) {
        RuntimeHandle::with_current(|runtime| {
            self.update_prg_banking(runtime);
            self.update_nametables(runtime);
        });
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

                let prg_bank_index = usize::from(byte & 0b0000_1111);
                let mirroring = if byte & 0b0001_0000 != 0 {
                    Mirroring::OneScreenUpper
                } else {
                    Mirroring::OneScreenLower
                };

                if prg_bank_index != self.state.prg_bank_index {
                    self.state.prg_bank_index = prg_bank_index;
                    self.update_prg_banking(runtime);
                }

                if mirroring != self.state.mirroring {
                    self.state.mirroring = mirroring;
                    self.update_nametables(runtime);
                }
            }
        });

        Ok(())
    }
}

#[derive(Debug)]
pub struct AxRomConfig {
    pub config: CartParams,
    pub bus_conflicts: bool,
}

impl<P: Platform> ComponentConfig<P> for AxRomConfig {
    type Component = AxRom;

    fn late_initialize(component: &mut Self::Component, _data: &LateContext<P>) {
        RuntimeHandle::with_current(|runtime| {
            component.update_prg_banking(runtime);
            component.update_nametables(runtime);
        });
    }

    fn build_component(
        self,
        mut component_builder: ComponentBuilder<P, Self::Component>,
    ) -> Result<Self::Component, Box<dyn core::error::Error>> {
        let prg_rom_size = self.config.prg_rom.len();

        if prg_rom_size == 0 || !prg_rom_size.is_multiple_of(PRG_BANK_SIZE) {
            return Err("AxROM PRG-ROM must be a non-zero multiple of 32KB".into());
        }

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

        Ok(AxRom {
            state: State {
                prg_bank_index: 0,
                mirroring: Mirroring::OneScreenLower,
            },
            config: self,
        })
    }
}
