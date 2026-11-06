use core::ops::RangeInclusive;

use alloc::boxed::Box;
use fluxemu_math::range::ContiguousRange;
use fluxemu_runtime::{
    RuntimeHandle,
    component::{
        Component,
        config::{ComponentConfig, LateContext},
    },
    machine::builder::ComponentBuilder,
    memory::{Address, AddressSpaceId, MapTarget, MemoryError, MemoryMapCommand, Permissions},
    platform::Platform,
};

use crate::cartridge::CartParams;

const CHR_BANK_SIZE: usize = 8 * 1024;

#[derive(Debug)]
pub struct CnRom {
    chr_bank_index: usize,
    config: CnRomConfig,
}

impl CnRom {
    fn update_chr_banking(&self, runtime: &RuntimeHandle) {
        let chr_rom = self.config.config.chr_rom.as_ref().unwrap();
        let chr_bank_count = chr_rom.len() / CHR_BANK_SIZE;

        runtime
            .address_space(self.config.config.ppu_address_space)
            .unwrap()
            .remap(
                runtime.current_timestamp(),
                [MemoryMapCommand::Map {
                    range: 0x0000..=0x1fff,
                    target: MapTarget::ImmutableMemory(chr_rom.slice(
                        RangeInclusive::from_start_and_length(
                            (self.chr_bank_index % chr_bank_count) * CHR_BANK_SIZE,
                            CHR_BANK_SIZE,
                        ),
                    )),
                    permissions: Permissions::READ,
                }],
            );
    }
}

impl Component for CnRom {
    type Event = ();

    fn memory_write(
        &mut self,
        _address: Address,
        _address_space: AddressSpaceId,
        buffer: &[u8],
    ) -> Result<(), MemoryError> {
        RuntimeHandle::with_current(|runtime| {
            let chr_bank_index = usize::from(buffer[0]);

            if chr_bank_index != self.chr_bank_index {
                self.chr_bank_index = chr_bank_index;
                self.update_chr_banking(runtime);
            }
        });

        Ok(())
    }
}

#[derive(Debug)]
pub struct CnRomConfig {
    pub config: CartParams,
}

impl<P: Platform> ComponentConfig<P> for CnRomConfig {
    type Component = CnRom;

    fn late_initialize(component: &mut Self::Component, _data: &LateContext<P>) {
        RuntimeHandle::with_current(|runtime| component.update_chr_banking(runtime));
    }

    fn build_component(
        self,
        component_builder: ComponentBuilder<P, Self::Component>,
    ) -> Result<Self::Component, Box<dyn core::error::Error>> {
        if self.config.chr_rom.is_none() {
            return Err("CNROM must have CHR-ROM".into());
        }

        let my_path = component_builder.path().clone();

        let component_builder = match self.config.prg_rom.len() {
            0x4000 => component_builder.map_memory(
                self.config.cpu_address_space,
                [
                    MemoryMapCommand::immutable_memory(0x8000, self.config.prg_rom.clone()),
                    MemoryMapCommand::mirror(Permissions::READ, 0xc000..=0xffff, 0x8000),
                ],
            ),
            0x8000 => component_builder.map_memory(
                self.config.cpu_address_space,
                [MemoryMapCommand::immutable_memory(
                    0x8000,
                    self.config.prg_rom.clone(),
                )],
            ),
            _ => return Err("Unsupported PRG ROM size for CNROM mapper".into()),
        };

        component_builder.map_memory(
            self.config.cpu_address_space,
            [MemoryMapCommand::Map {
                range: 0x8000..=0xffff,
                permissions: Permissions::WRITE,
                target: MapTarget::Component(my_path),
            }],
        );

        Ok(CnRom {
            chr_bank_index: 0,
            config: self,
        })
    }
}
