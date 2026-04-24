use fluxemu_runtime::{
    component::{Component, config::ComponentConfig},
    machine::builder::ComponentBuilder,
    persistence::PersistanceFormatVersion,
    platform::Platform,
};

use crate::cartridge::CartParams;

#[derive(Debug)]
pub struct NRom;

impl Component for NRom {
    type Event = ();

    fn load_snapshot(
        &mut self,
        _version: PersistanceFormatVersion,
        _reader: &mut dyn std::io::Read,
    ) -> Result<(), Box<dyn std::error::Error>> {
        Ok(())
    }

    fn store_snapshot(
        &self,
        _writer: &mut dyn std::io::Write,
    ) -> Result<(), Box<dyn std::error::Error>> {
        Ok(())
    }
}

#[derive(Debug)]
pub struct NRomConfig {
    pub config: CartParams,
}

impl<P: Platform> ComponentConfig<P> for NRomConfig {
    type Component = NRom;
    const CURRENT_SNAPSHOT_VERSION: PersistanceFormatVersion = 0;

    fn build_component(
        self,
        component_builder: ComponentBuilder<'_, '_, P, Self::Component>,
    ) -> Result<Self::Component, Box<dyn std::error::Error>> {
        let prg_bank_count = self.config.prg_rom.len() / (16 * 1024);

        let component_builder = match prg_bank_count {
            // NROM-128
            1 => {
                let component_builder = component_builder.memory_map_buffer_read(
                    self.config.cpu_address_space,
                    0x8000..=0xbfff,
                    self.config.prg_rom,
                );

                component_builder.memory_mirror_map_read(
                    self.config.cpu_address_space,
                    0xc000..=0xffff,
                    0x8000..=0xbfff,
                )
            }
            // NROM-256
            2 => component_builder.memory_map_buffer_read(
                self.config.cpu_address_space,
                0x8000..=0xffff,
                self.config.prg_rom,
            ),
            _ => return Err("Unsupported PRG ROM size for NROM mapper".into()),
        };

        component_builder.memory_map_buffer_read(
            self.config.ppu_address_space,
            0x0000..=0x1fff,
            self.config.chr_rom.ok_or("NROM must have CHR-ROM")?,
        );

        Ok(NRom)
    }
}
