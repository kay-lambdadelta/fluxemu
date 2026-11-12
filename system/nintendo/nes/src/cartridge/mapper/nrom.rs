use alloc::boxed::Box;

use fluxemu_platform::Platform;
use fluxemu_runtime::{
    component::{Component, config::ComponentConfig},
    machine::builder::ComponentBuilder,
    memory::{MemoryMapCommand, Permissions},
};

use crate::cartridge::CartParams;

#[derive(Debug)]
pub struct NRom;

impl Component for NRom {
    type Event = ();
    type Save = ();
    type Snapshot = ();
}

#[derive(Debug)]
pub struct NRomConfig {
    pub config: CartParams,
}

impl<P: Platform> ComponentConfig<P> for NRomConfig {
    type Component = NRom;

    fn build_component(
        self,
        component_builder: ComponentBuilder<P, Self::Component>,
    ) -> Result<Self::Component, Box<dyn core::error::Error>> {
        let prg_bank_count = self.config.prg_rom.len() / (16 * 1024);

        let component_builder = match prg_bank_count {
            // NROM-128
            1 => {
                if self.config.prg_rom.len() != 16 * 1024 {
                    return Err("NROM-128 must have exactly 16KB of PRG-ROM".into());
                }

                component_builder.map_memory(
                    self.config.cpu_address_space,
                    [
                        MemoryMapCommand::immutable_memory(0x8000, self.config.prg_rom.clone()),
                        MemoryMapCommand::mirror(Permissions::READ, 0xc000..=0xffff, 0x8000),
                    ],
                )
            }
            // NROM-256
            2 => {
                if self.config.prg_rom.len() != 32 * 1024 {
                    return Err("NROM-256 must have exactly 32KB of PRG-ROM".into());
                }

                component_builder.map_memory(
                    self.config.cpu_address_space,
                    [MemoryMapCommand::immutable_memory(
                        0x8000,
                        self.config.prg_rom.clone(),
                    )],
                )
            }
            _ => return Err("Unsupported PRG ROM size for NROM mapper".into()),
        };

        self.config.map_fixed_chr(component_builder)?;

        Ok(NRom)
    }
}
