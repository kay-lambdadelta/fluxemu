use fluxemu_runtime::{
    component::{Component, config::ComponentConfig},
    machine::builder::ComponentBuilder,
    memory::{MemoryMapCommand, Permissions},
    platform::Platform,
};

use crate::cartridge::CartParams;

#[derive(Debug)]
pub struct NRom;

impl Component for NRom {
    type Event = ();
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
    ) -> Result<Self::Component, Box<dyn std::error::Error>> {
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
                        MemoryMapCommand::immutable_memory(0x8000, self.config.prg_rom),
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
                        self.config.prg_rom,
                    )],
                )
            }
            _ => return Err("Unsupported PRG ROM size for NROM mapper".into()),
        };

        let chr_rom = self.config.chr_rom.ok_or("NROM must have CHR-ROM")?;
        if chr_rom.len() != 0x2000 {
            return Err("CHR-ROM must have exactly 8KB of CHR-ROM".into());
        }

        component_builder.map_memory(
            self.config.ppu_address_space,
            [MemoryMapCommand::immutable_memory(0x0000, chr_rom)],
        );

        Ok(NRom)
    }
}
