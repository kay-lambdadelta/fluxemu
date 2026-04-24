use std::{io::Read, ops::RangeInclusive};

use fluxemu_range::ContiguousRange;
use fluxemu_runtime::{
    ComponentPath, ComponentRuntimeApi,
    component::{
        Component,
        config::{ComponentConfig, LateContext},
    },
    machine::builder::ComponentBuilder,
    memory::{
        Address, AddressSpaceId, MapTarget, MemoryError, MemoryRemappingCommand, Permissions,
        component::{InitialContents, MemoryConfig},
    },
    persistence::PersistanceFormatVersion,
    platform::Platform,
};
use rangemap::RangeInclusiveMap;

use crate::{
    cartridge::{CartParams, mapper::mmc1::shift::ShiftRegister},
    ppu::NAMETABLE_ADDRESSES,
};

mod shift;

const PRG_BANK_SIZE: usize = 16 * 1024;
const CHR_BANK_SIZE: usize = 4 * 1024;

#[derive(Debug, PartialEq, Eq)]
enum PrgRomBankMode {
    Unified32k,
    LockFirstBank,
    LockLastBank,
}

#[derive(Debug, PartialEq, Eq)]
enum ChrRomBankMode {
    Unified8k,
    Split4k,
}

#[derive(Debug, PartialEq, Eq)]
enum Mirroring {
    OneScreenLower,
    OneScreenUpper,
    Vertical,
    Horizontal,
}

#[derive(Debug)]
pub struct Mmc1 {
    shift_register: ShiftRegister,
    chr_rom_bank_mode: ChrRomBankMode,
    chr_rom_bank_indexes: [u8; 2],
    prg_rom_bank_mode: PrgRomBankMode,
    prg_rom_bank_index: u8,
    mirroring: Mirroring,
    config: Mmc1Config,
    path: ComponentPath,
}

impl Mmc1 {
    fn update_banking(&mut self) {
        // Note that this is temporally sound to do both as seperate operations because
        //
        // 1: Remappings can only be triggered by the cpu
        // 2: The ppu cannot overrun the cpu's timestamp
        //
        // Therefore the ppu can never observe stale mappings

        let runtime = ComponentRuntimeApi::current(&self.path);
        let timestamp = runtime.current_timestamp();

        let mut cpu_commands = Vec::new();
        let mut ppu_commands = Vec::new();

        let (prg_low_bank, prg_high_bank) = match self.prg_rom_bank_mode {
            PrgRomBankMode::Unified32k => {
                let bank = (self.prg_rom_bank_index & 0b1111_1110) as usize;

                (bank, bank + 1)
            }
            PrgRomBankMode::LockFirstBank => (0, self.prg_rom_bank_index as usize),
            PrgRomBankMode::LockLastBank => {
                let last = (self.config.params.prg_rom.len() / PRG_BANK_SIZE) - 1;

                (self.prg_rom_bank_index as usize, last)
            }
        };

        cpu_commands.push(MemoryRemappingCommand::Map {
            range: 0x8000..=0xbfff,
            target: MapTarget::Buffer(self.config.params.prg_rom.slice(
                RangeInclusive::from_start_and_length(prg_low_bank * PRG_BANK_SIZE, PRG_BANK_SIZE),
            )),
            permissions: Permissions::READ,
        });

        cpu_commands.push(MemoryRemappingCommand::Map {
            range: 0xc000..=0xffff,
            target: MapTarget::Buffer(self.config.params.prg_rom.slice(
                RangeInclusive::from_start_and_length(prg_high_bank * PRG_BANK_SIZE, PRG_BANK_SIZE),
            )),
            permissions: Permissions::READ,
        });

        if let Some(chr_rom) = self.config.params.chr_rom.as_ref() {
            match self.chr_rom_bank_mode {
                ChrRomBankMode::Unified8k => {
                    let bank = (self.chr_rom_bank_indexes[0] & !1) as usize;

                    ppu_commands.push(MemoryRemappingCommand::Map {
                        range: 0x0000..=0x0fff,
                        target: MapTarget::Buffer(chr_rom.slice(
                            RangeInclusive::from_start_and_length(
                                bank * CHR_BANK_SIZE,
                                CHR_BANK_SIZE,
                            ),
                        )),
                        permissions: Permissions::READ,
                    });

                    ppu_commands.push(MemoryRemappingCommand::Map {
                        range: 0x1000..=0x1fff,
                        target: MapTarget::Buffer(chr_rom.slice(
                            RangeInclusive::from_start_and_length(
                                (bank + 1) * CHR_BANK_SIZE,
                                CHR_BANK_SIZE,
                            ),
                        )),
                        permissions: Permissions::READ,
                    });
                }
                ChrRomBankMode::Split4k => {
                    for (i, &bank_index) in self.chr_rom_bank_indexes.iter().enumerate() {
                        let ppu_base = i * 0x1000;
                        let rom_offset = bank_index as usize * CHR_BANK_SIZE;

                        ppu_commands.push(MemoryRemappingCommand::Map {
                            range: RangeInclusive::from_start_and_length(ppu_base, 0x1000),
                            target: MapTarget::Buffer(chr_rom.slice(
                                RangeInclusive::from_start_and_length(rom_offset, CHR_BANK_SIZE),
                            )),
                            permissions: Permissions::READ,
                        });
                    }
                }
            }
        }

        runtime
            .address_space(self.config.params.cpu_address_space)
            .unwrap()
            .remap(timestamp, cpu_commands);

        runtime
            .address_space(self.config.params.ppu_address_space)
            .unwrap()
            .remap(timestamp, ppu_commands);
    }

    fn update_nametables(&mut self) {
        let runtime = ComponentRuntimeApi::current(&self.path);
        let timestamp = runtime.current_timestamp();

        let [nametable_0, nametable_1] = &self.config.params.nametables;

        let commands = match self.mirroring {
            Mirroring::OneScreenLower => vec![
                MemoryRemappingCommand::Map {
                    range: NAMETABLE_ADDRESSES[0].clone(),
                    target: MapTarget::Component(nametable_0.clone()),
                    permissions: Permissions::ALL,
                },
                MemoryRemappingCommand::RebaseComponent {
                    component: nametable_0.clone(),
                    base: *NAMETABLE_ADDRESSES[0].start(),
                },
                MemoryRemappingCommand::Map {
                    range: NAMETABLE_ADDRESSES[1].clone(),
                    target: MapTarget::Mirror {
                        destination: NAMETABLE_ADDRESSES[0].clone(),
                    },
                    permissions: Permissions::ALL,
                },
                MemoryRemappingCommand::Map {
                    range: NAMETABLE_ADDRESSES[2].clone(),
                    target: MapTarget::Mirror {
                        destination: NAMETABLE_ADDRESSES[0].clone(),
                    },
                    permissions: Permissions::ALL,
                },
                MemoryRemappingCommand::Map {
                    range: NAMETABLE_ADDRESSES[3].clone(),
                    target: MapTarget::Mirror {
                        destination: NAMETABLE_ADDRESSES[0].clone(),
                    },
                    permissions: Permissions::ALL,
                },
            ],
            Mirroring::OneScreenUpper => vec![
                MemoryRemappingCommand::Map {
                    range: NAMETABLE_ADDRESSES[0].clone(),
                    target: MapTarget::Mirror {
                        destination: NAMETABLE_ADDRESSES[1].clone(),
                    },
                    permissions: Permissions::ALL,
                },
                MemoryRemappingCommand::Map {
                    range: NAMETABLE_ADDRESSES[1].clone(),
                    target: MapTarget::Component(nametable_1.clone()),
                    permissions: Permissions::ALL,
                },
                MemoryRemappingCommand::RebaseComponent {
                    component: nametable_1.clone(),
                    base: *NAMETABLE_ADDRESSES[1].start(),
                },
                MemoryRemappingCommand::Map {
                    range: NAMETABLE_ADDRESSES[2].clone(),
                    target: MapTarget::Mirror {
                        destination: NAMETABLE_ADDRESSES[1].clone(),
                    },
                    permissions: Permissions::ALL,
                },
                MemoryRemappingCommand::Map {
                    range: NAMETABLE_ADDRESSES[3].clone(),
                    target: MapTarget::Mirror {
                        destination: NAMETABLE_ADDRESSES[1].clone(),
                    },
                    permissions: Permissions::ALL,
                },
            ],
            Mirroring::Vertical => vec![
                MemoryRemappingCommand::Map {
                    range: NAMETABLE_ADDRESSES[0].clone(),
                    target: MapTarget::Component(nametable_0.clone()),
                    permissions: Permissions::ALL,
                },
                MemoryRemappingCommand::RebaseComponent {
                    component: nametable_0.clone(),
                    base: *NAMETABLE_ADDRESSES[0].start(),
                },
                MemoryRemappingCommand::Map {
                    range: NAMETABLE_ADDRESSES[1].clone(),
                    target: MapTarget::Component(nametable_1.clone()),
                    permissions: Permissions::ALL,
                },
                MemoryRemappingCommand::RebaseComponent {
                    component: nametable_1.clone(),
                    base: *NAMETABLE_ADDRESSES[1].start(),
                },
                MemoryRemappingCommand::Map {
                    range: NAMETABLE_ADDRESSES[2].clone(),
                    target: MapTarget::Mirror {
                        destination: NAMETABLE_ADDRESSES[0].clone(),
                    },
                    permissions: Permissions::ALL,
                },
                MemoryRemappingCommand::Map {
                    range: NAMETABLE_ADDRESSES[3].clone(),
                    target: MapTarget::Mirror {
                        destination: NAMETABLE_ADDRESSES[1].clone(),
                    },
                    permissions: Permissions::ALL,
                },
            ],
            Mirroring::Horizontal => vec![
                MemoryRemappingCommand::Map {
                    range: NAMETABLE_ADDRESSES[0].clone(),
                    target: MapTarget::Component(nametable_0.clone()),
                    permissions: Permissions::ALL,
                },
                MemoryRemappingCommand::RebaseComponent {
                    component: nametable_0.clone(),
                    base: *NAMETABLE_ADDRESSES[0].start(),
                },
                MemoryRemappingCommand::Map {
                    range: NAMETABLE_ADDRESSES[1].clone(),
                    target: MapTarget::Mirror {
                        destination: NAMETABLE_ADDRESSES[0].clone(),
                    },
                    permissions: Permissions::ALL,
                },
                MemoryRemappingCommand::Map {
                    range: NAMETABLE_ADDRESSES[2].clone(),
                    target: MapTarget::Component(nametable_1.clone()),
                    permissions: Permissions::ALL,
                },
                MemoryRemappingCommand::RebaseComponent {
                    component: nametable_1.clone(),
                    base: *NAMETABLE_ADDRESSES[2].start(),
                },
                MemoryRemappingCommand::Map {
                    range: NAMETABLE_ADDRESSES[3].clone(),
                    target: MapTarget::Mirror {
                        destination: NAMETABLE_ADDRESSES[2].clone(),
                    },
                    permissions: Permissions::ALL,
                },
            ],
        };

        runtime
            .address_space(self.config.params.ppu_address_space)
            .unwrap()
            .remap(timestamp, commands);
    }
}

impl Component for Mmc1 {
    type Event = ();

    fn load_snapshot(
        &mut self,
        _version: PersistanceFormatVersion,
        _reader: &mut dyn Read,
    ) -> Result<(), Box<dyn std::error::Error>> {
        todo!()
    }

    fn store_snapshot(
        &self,
        _writer: &mut dyn std::io::Write,
    ) -> Result<(), Box<dyn std::error::Error>> {
        todo!()
    }

    fn memory_write(
        &mut self,
        address: Address,
        _address_space: AddressSpaceId,
        buffer: &[u8],
    ) -> Result<(), MemoryError> {
        for (address, byte) in
            RangeInclusive::from_start_and_length(address, buffer.len()).zip(buffer.iter().copied())
        {
            if (0x8000..=0xffff).contains(&address) {
                let shift_in_bit = byte & 0b0000_0001 != 0;
                let reset = byte & 0b1000_0000 != 0;

                if reset {
                    self.shift_register = ShiftRegister::default();
                    self.prg_rom_bank_mode = PrgRomBankMode::LockLastBank;

                    self.update_banking();
                    continue;
                }

                if let Some(value) = self.shift_register.shift(shift_in_bit) {
                    let remap;

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

                            remap = chr_rom_bank_mode != self.chr_rom_bank_mode
                                || prg_rom_bank_mode != self.prg_rom_bank_mode
                                || mirroring != self.mirroring;

                            self.chr_rom_bank_mode = chr_rom_bank_mode;
                            self.prg_rom_bank_mode = prg_rom_bank_mode;
                            self.mirroring = mirroring;
                        }
                        0xa000..=0xbfff => {
                            let index = value & 0b0001_1111;

                            remap = index != self.chr_rom_bank_indexes[0];

                            self.chr_rom_bank_indexes[0] = index;
                        }
                        0xc000..=0xdfff => {
                            let index = value & 0b0001_1111;

                            remap = index != self.chr_rom_bank_indexes[1];

                            self.chr_rom_bank_indexes[1] = index;
                        }
                        0xe000..=0xffff => {
                            let index = value & 0b0000_1111;

                            remap = index != self.prg_rom_bank_index;

                            self.prg_rom_bank_index = index;
                        }
                        _ => unreachable!(),
                    }

                    if remap {
                        self.update_banking();
                        self.update_nametables();
                    }
                }
            }
        }

        Ok(())
    }
}

#[derive(Debug)]
pub struct Mmc1Config {
    pub params: CartParams,
}

impl<P: Platform> ComponentConfig<P> for Mmc1Config {
    type Component = Mmc1;
    const CURRENT_SNAPSHOT_VERSION: PersistanceFormatVersion = 0;

    fn late_initialize(component: &mut Self::Component, _data: &LateContext<P>) {
        // Force the system to adopt an initial mapping
        component.update_banking();
    }

    fn build_component(
        self,
        mut component_builder: ComponentBuilder<'_, '_, P, Self::Component>,
    ) -> Result<Self::Component, Box<dyn std::error::Error>> {
        if self.params.chr_rom.is_some()
            && (self.params.chr_ram_size != 0 || self.params.chr_nvram_size != 0)
        {
            return Err(
                "Cartridge has both CHR-ROM and CHR-RAM, which are mutually exclusive for MMC1"
                    .into(),
            );
        }

        if self.params.chr_rom.is_none()
            && self.params.chr_ram_size == 0
            && self.params.chr_nvram_size == 0
        {
            return Err("Cartridge has neither CHR-ROM nor CHR-RAM".into());
        }

        if self.params.chr_rom.is_none() {
            let (size, sram) = if self.params.chr_nvram_size > 0 {
                (self.params.chr_nvram_size, true)
            } else {
                (self.params.chr_ram_size, false)
            };

            let (cb, _) = component_builder.component(
                "chr-ram",
                MemoryConfig {
                    readable: true,
                    writable: true,
                    assigned_range: RangeInclusive::from_start_and_length(0x0000, size),
                    assigned_address_space: self.params.ppu_address_space,
                    initial_contents: RangeInclusiveMap::from_iter([(
                        RangeInclusive::from_start_and_length(0x0000, size),
                        InitialContents::Random,
                    )]),
                    sram,
                },
            );

            component_builder = cb;
        }

        if self.params.prg_ram_size != 0 {
            if self.params.prg_ram_size != 8 * 1024 {
                return Err("PRG-RAM size is invalid for MMC1".into());
            }

            let (cb, _) = component_builder.component(
                "prg-ram",
                MemoryConfig {
                    readable: true,
                    writable: true,
                    assigned_range: 0x6000..=0x7fff,
                    assigned_address_space: self.params.cpu_address_space,
                    initial_contents: RangeInclusiveMap::from_iter([(
                        0x6000..=0x7fff,
                        InitialContents::Random,
                    )]),
                    sram: true,
                },
            );

            component_builder = cb;
        }

        let my_path = component_builder.path().clone();

        // Control register
        component_builder
            .memory_map_component_write(self.params.cpu_address_space, 0x8000..=0xffff);

        Ok(Mmc1 {
            shift_register: ShiftRegister::default(),
            config: self,
            chr_rom_bank_mode: ChrRomBankMode::Unified8k,
            chr_rom_bank_indexes: [0, 0],
            prg_rom_bank_mode: PrgRomBankMode::LockLastBank,
            prg_rom_bank_index: 0,
            mirroring: Mirroring::Horizontal,
            path: my_path,
        })
    }
}
