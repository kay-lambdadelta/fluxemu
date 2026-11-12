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

use crate::cartridge::{CartParams, Mirroring, mapper::mmc1::shift::ShiftRegister, rom_bank};

mod shift;

const PRG_BANK_SIZE: usize = 16 * 1024;
const CHR_BANK_SIZE: usize = 4 * 1024;

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
enum PrgRomBankMode {
    Unified32k,
    LockFirstBank,
    LockLastBank,
}

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
enum ChrRomBankMode {
    Unified8k,
    Split4k,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct State {
    shift_register: ShiftRegister,
    chr_rom_bank_mode: ChrRomBankMode,
    chr_rom_bank_indexes: [u8; 2],
    prg_rom_bank_mode: PrgRomBankMode,
    prg_rom_bank_index: u8,
    mirroring: Option<Mirroring>,
}

#[derive(Debug)]
pub struct Mmc1 {
    state: State,
    config: Mmc1Config,
}

impl PersistentComponentState<Mmc1> for State {
    fn capture(component: &Mmc1) -> impl Serialize {
        &component.state
    }

    fn apply(self, component: &mut Mmc1) {
        component.state = self;
    }
}

impl Mmc1 {
    fn update_prg_banking(&mut self, runtime: &RuntimeHandle) {
        // Note that this is temporally sound to do these as seperate operations because
        //
        // 1: Remappings can only be triggered by the cpu
        // 2: The ppu cannot overrun the cpu's timestamp
        //
        // Therefore the ppu can never observe stale mappings

        let timestamp = runtime.current_timestamp();

        let prg_bank_count = self.config.params.prg_rom.len() / PRG_BANK_SIZE;

        let (prg_low_bank, prg_high_bank) = match self.state.prg_rom_bank_mode {
            PrgRomBankMode::Unified32k => {
                let bank = (self.state.prg_rom_bank_index & 0b1111_1110) as usize;

                (bank, bank + 1)
            }
            PrgRomBankMode::LockFirstBank => (0, self.state.prg_rom_bank_index as usize),
            PrgRomBankMode::LockLastBank => {
                (self.state.prg_rom_bank_index as usize, prg_bank_count - 1)
            }
        };

        let prg_rom = &self.config.params.prg_rom;
        let cpu_commands = [
            rom_bank(prg_rom, 0x8000, PRG_BANK_SIZE, prg_low_bank),
            rom_bank(prg_rom, 0xc000, PRG_BANK_SIZE, prg_high_bank),
        ];

        runtime
            .address_space(self.config.params.cpu_address_space)
            .unwrap()
            .remap(timestamp, cpu_commands);
    }

    fn update_chr_banking(&mut self, runtime: &RuntimeHandle) {
        let timestamp = runtime.current_timestamp();

        if let Some(chr_rom) = self.config.params.chr_rom.as_ref() {
            let [low_bank, high_bank] = match self.state.chr_rom_bank_mode {
                ChrRomBankMode::Unified8k => {
                    let bank = (self.state.chr_rom_bank_indexes[0] & !1) as usize;

                    [bank, bank + 1]
                }
                ChrRomBankMode::Split4k => self.state.chr_rom_bank_indexes.map(usize::from),
            };

            let ppu_commands = [
                rom_bank(chr_rom, 0x0000, CHR_BANK_SIZE, low_bank),
                rom_bank(chr_rom, 0x1000, CHR_BANK_SIZE, high_bank),
            ];

            runtime
                .address_space(self.config.params.ppu_address_space)
                .unwrap()
                .remap(timestamp, ppu_commands);
        }
    }

    fn update_nametables(&mut self, runtime: &RuntimeHandle) {
        let Some(mirroring) = self.state.mirroring else {
            return;
        };

        let timestamp = runtime.current_timestamp();

        let commands = mirroring.nametable_commands(&self.config.params.nametables);

        runtime
            .address_space(self.config.params.ppu_address_space)
            .unwrap()
            .remap(timestamp, commands);
    }
}

impl Component for Mmc1 {
    type Event = ();
    type Save = ();
    type Snapshot = State;

    fn state_restored(&mut self) {
        RuntimeHandle::with_current(|runtime| {
            self.update_prg_banking(runtime);
            self.update_chr_banking(runtime);
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
                if (0x8000..=0xffff).contains(&address) {
                    let shift_in_bit = byte & 0b0000_0001 != 0;
                    let reset = byte & 0b1000_0000 != 0;

                    if reset {
                        self.state.shift_register = ShiftRegister::default();

                        if self.state.prg_rom_bank_mode != PrgRomBankMode::LockLastBank {
                            self.state.prg_rom_bank_mode = PrgRomBankMode::LockLastBank;
                            self.update_prg_banking(runtime);
                        }

                        continue;
                    }

                    if let Some(value) = self.state.shift_register.shift(shift_in_bit) {
                        let mut remap_prg = false;
                        let mut remap_chr = false;
                        let mut remap_nametables = false;

                        match address {
                            0x8000..=0x9fff => {
                                let chr_rom_bank_mode = if value & 0b0001_0000 != 0 {
                                    ChrRomBankMode::Split4k
                                } else {
                                    ChrRomBankMode::Unified8k
                                };

                                let prg_rom_bank_mode = match (value & 0b0000_1100) >> 2 {
                                    0 | 1 => PrgRomBankMode::Unified32k,
                                    2 => PrgRomBankMode::LockFirstBank,
                                    3 => PrgRomBankMode::LockLastBank,
                                    _ => unreachable!(),
                                };

                                let mirroring = match value & 0b0000_0011 {
                                    0 => Mirroring::OneScreenLower,
                                    1 => Mirroring::OneScreenUpper,
                                    2 => Mirroring::Vertical,
                                    3 => Mirroring::Horizontal,
                                    _ => unreachable!(),
                                };

                                remap_chr = chr_rom_bank_mode != self.state.chr_rom_bank_mode;
                                remap_prg = prg_rom_bank_mode != self.state.prg_rom_bank_mode;
                                remap_nametables =
                                    self.state.mirroring.as_ref() != Some(&mirroring);

                                self.state.chr_rom_bank_mode = chr_rom_bank_mode;
                                self.state.prg_rom_bank_mode = prg_rom_bank_mode;
                                self.state.mirroring = Some(mirroring);
                            }
                            0xa000..=0xbfff => {
                                let index = value & 0b0001_1111;

                                remap_chr = index != self.state.chr_rom_bank_indexes[0];

                                self.state.chr_rom_bank_indexes[0] = index;
                            }
                            0xc000..=0xdfff => {
                                let index = value & 0b0001_1111;

                                remap_chr = index != self.state.chr_rom_bank_indexes[1]
                                    && self.state.chr_rom_bank_mode == ChrRomBankMode::Split4k;

                                self.state.chr_rom_bank_indexes[1] = index;
                            }
                            0xe000..=0xffff => {
                                let index = value & 0b0000_1111;

                                remap_prg = index != self.state.prg_rom_bank_index;

                                self.state.prg_rom_bank_index = index;
                            }
                            _ => unreachable!(),
                        }

                        if remap_prg {
                            self.update_prg_banking(runtime);
                        }

                        if remap_chr {
                            self.update_chr_banking(runtime);
                        }

                        if remap_nametables {
                            self.update_nametables(runtime);
                        }
                    }
                }
            }
        });

        Ok(())
    }
}

#[derive(Debug)]
pub struct Mmc1Config {
    pub params: CartParams,
}

impl<P: Platform> ComponentConfig<P> for Mmc1Config {
    type Component = Mmc1;

    fn late_initialize(component: &mut Self::Component, _data: &LateContext<P>) {
        // Force the system to adopt an initial mapping
        RuntimeHandle::with_current(|runtime| {
            component.update_prg_banking(runtime);
            component.update_chr_banking(runtime);
        });
    }

    fn build_component(
        self,
        mut component_builder: ComponentBuilder<P, Self::Component>,
    ) -> Result<Self::Component, Box<dyn core::error::Error>> {
        if self.params.chr_rom.is_some()
            && (self.params.chr_ram_size != 0 || self.params.chr_nvram_size != 0)
        {
            return Err(
                "Cartridge has both CHR-ROM and CHR-RAM, which are mutually exclusive for MMC1"
                    .into(),
            );
        }

        if self.params.chr_rom.is_none() {
            component_builder = self.params.map_chr_ram(component_builder)?;
        }

        let prg_ram_size = self.params.prg_ram_size + self.params.prg_nvram_size;

        if prg_ram_size != 0 {
            if prg_ram_size != 0x2000 {
                return Err("PRG-RAM size is invalid for MMC1".into());
            }

            let (cb, prg_ram_path) = if self.params.prg_nvram_size > 0 {
                component_builder.save_memory("prg-ram", 0x2000, [])
            } else {
                component_builder.memory("prg-ram", 0x2000, [])
            };
            let cb = cb.map_memory(
                self.params.cpu_address_space,
                [MemoryMapCommand::Map {
                    range: RangeInclusive::from_start_and_length(0x6000, 0x2000),
                    permissions: Permissions::ALL,
                    target: MapTarget::Memory {
                        path: prg_ram_path,
                        subrange: None,
                    },
                }],
            );

            component_builder = cb;
        }

        let my_path = component_builder.path().clone();

        // Control register
        component_builder.map_memory(
            self.params.cpu_address_space,
            [MemoryMapCommand::Map {
                range: 0x8000..=0xffff,
                permissions: Permissions::WRITE,
                target: MapTarget::Component(my_path.clone()),
            }],
        );

        Ok(Mmc1 {
            state: State {
                shift_register: ShiftRegister::default(),
                chr_rom_bank_mode: ChrRomBankMode::Unified8k,
                chr_rom_bank_indexes: [0, 0],
                prg_rom_bank_mode: PrgRomBankMode::LockLastBank,
                prg_rom_bank_index: 0,
                mirroring: None,
            },
            config: self,
        })
    }
}
