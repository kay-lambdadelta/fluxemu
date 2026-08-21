use std::{marker::PhantomData, ops::RangeInclusive};

pub use cartridge::ines::INes;
use cartridge::{CartParams, ines::TimingMode};
use fluxemu_definition_mos6502::variant::Ricoh2A0x;
use fluxemu_math::range::ContiguousRange;
use fluxemu_program::{NintendoSystem, SystemId};
use fluxemu_runtime::{
    ResourcePath,
    machine::builder::{MachineBuilder, RomRequirement, SealedMachineBuilder},
    memory::{AddressSpaceId, MapTarget, MemoryMapCommand, Permissions},
    platform::Platform,
};
use fluxemu_system::System;
use ppu::PpuConfig;

use crate::{
    apu::ApuConfig,
    cartridge::{
        ines::{INesVersion, NametableMirroring, expansion_device::DefaultExpansionDevice},
        mapper::{mmc1::Mmc1Config, nrom::NRomConfig},
    },
    gamepad::standard_controllers::NesControllerConfig,
    ppu::{
        BACKGROUND_PALETTE_BASE_ADDRESS, NAMETABLE_ADDRESSES, PALETTE_RAM_ADDRESSES,
        backend::SupportedGraphicsApiPpu,
        region::{Region, ntsc::Ntsc, pal::Pal},
    },
};

mod apu;
mod cartridge;
mod gamepad;
mod ppu;

#[derive(Debug, Default)]
pub struct Nes;

impl<G: SupportedGraphicsApiPpu, P: Platform<GraphicsApi = G>> System<P> for Nes {
    type Quirks = ();

    const ID: SystemId = SystemId::Nintendo(NintendoSystem::NintendoEntertainmentSystem);

    fn build(
        &self,
        _quirks: Self::Quirks,
        machine_builder: MachineBuilder<P>,
    ) -> SealedMachineBuilder<P> {
        let (machine_builder, cpu_address_space) = machine_builder.address_space(16);
        let (machine_builder, ppu_address_space) = machine_builder.address_space(14);

        let program_specification = machine_builder.program_specification().unwrap();
        let filesystem = program_specification.info.filesystem();

        assert_eq!(
            filesystem.len(),
            1,
            "iNES format NES programs only contain a single ROM"
        );

        let rom_id = filesystem
            .first_key_value()
            .map(|(rom_id, _)| rom_id)
            .copied()
            .unwrap();

        let work_ram_range = RangeInclusive::from_start_and_length(0usize, 0x800);

        let (machine_builder, work_ram_path) =
            machine_builder.memory("work-ram", work_ram_range.len(), []);
        let machine_builder = machine_builder.map_memory(
            cpu_address_space,
            [MemoryMapCommand::Map {
                range: work_ram_range.clone(),
                permissions: Permissions::ALL,
                target: MapTarget::Memory {
                    path: work_ram_path,
                    subrange: None,
                },
            }],
        );

        let (machine_builder, palette_ram_path) = machine_builder.memory("palette-ram", 0x20, []);
        let machine_builder = machine_builder.map_memory(
            ppu_address_space,
            [MemoryMapCommand::Map {
                range: PALETTE_RAM_ADDRESSES,
                permissions: Permissions::ALL,
                target: MapTarget::Memory {
                    path: palette_ram_path,
                    subrange: None,
                },
            }],
        );

        // Workram mirrors
        let machine_builder = machine_builder.map_memory(
            cpu_address_space,
            MemoryMapCommand::with_mirrors_to_destination(
                work_ram_range.clone(),
                [
                    (0x0800, Permissions::ALL),
                    (0x1000, Permissions::ALL),
                    (0x1800, Permissions::ALL),
                ],
            ),
        );

        // Background palette mirrors
        let machine_builder = machine_builder.map_memory(
            ppu_address_space,
            [
                MemoryMapCommand::mirror(
                    Permissions::ALL,
                    RangeInclusive::from_single(0x3f10),
                    0x3f00,
                ),
                MemoryMapCommand::mirror(
                    Permissions::ALL,
                    RangeInclusive::from_single(0x3f14),
                    0x3f04,
                ),
                MemoryMapCommand::mirror(
                    Permissions::ALL,
                    RangeInclusive::from_single(0x3f18),
                    0x3f08,
                ),
                MemoryMapCommand::mirror(
                    Permissions::ALL,
                    RangeInclusive::from_single(0x3f1c),
                    0x3f0c,
                ),
            ],
        );

        // full palette mirror blocks
        let machine_builder = machine_builder
            .map_memory(
                ppu_address_space,
                MemoryMapCommand::with_mirrors_to_destination(
                    PALETTE_RAM_ADDRESSES,
                    RangeInclusive::from_start_and_length(BACKGROUND_PALETTE_BASE_ADDRESS, 0x100)
                        .step_by(0x20)
                        .skip(1)
                        .map(|address| (address, Permissions::ALL)),
                ),
            )
            .map_memory(
                cpu_address_space,
                MemoryMapCommand::with_mirrors_to_destination(
                    RangeInclusive::from_start_and_length(0x2000, 8),
                    RangeInclusive::from_start_and_length(0x2000, 0x2000)
                        .step_by(8)
                        .skip(1)
                        .map(|address| (address, Permissions::ALL)),
                ),
            );

        let rom = machine_builder
            .open_rom(rom_id, RomRequirement::Required)
            .unwrap()
            .unwrap();

        let header = INes::parse(rom[0..16].try_into().unwrap()).unwrap();
        if header.trainer {
            tracing::warn!("This ROM contains a trainer, which is not emulated at this time");
        }

        let (machine_builder, nametable_0) =
            machine_builder.memory("nametable-0", NAMETABLE_ADDRESSES[0].len(), []);
        let (machine_builder, nametable_1) =
            machine_builder.memory("nametable-1", NAMETABLE_ADDRESSES[0].len(), []);

        let nametables = [nametable_0, nametable_1];

        let machine_builder =
            setup_ppu_nametables(machine_builder, ppu_address_space, &nametables, &header);

        // Nametable mirror
        let mut machine_builder = machine_builder.map_memory(
            ppu_address_space,
            [MemoryMapCommand::mirror(
                Permissions::ALL,
                0x3000..=0x3eff,
                0x2000,
            )],
        );

        let prg_rom = header.extract_prg_rom(&rom);
        let chr_rom = header.extract_chr_rom(&rom);

        let cart_config = CartParams {
            cpu_address_space,
            ppu_address_space,
            chr_rom,
            prg_rom,
            chr_ram_size: header.chr_ram_size,
            chr_nvram_size: header.chr_nvram_size,
            prg_ram_size: header.prg_ram_size,
            nametables,
        };

        #[allow(clippy::zero_prefixed_literal)]
        match header.mapper {
            000 => {
                machine_builder = machine_builder
                    .component(
                        "nrom_cartridge",
                        NRomConfig {
                            config: cart_config,
                        },
                    )
                    .0;
            }
            001 | 155 => {
                machine_builder = machine_builder
                    .component(
                        "mmc1_cartridge",
                        Mmc1Config {
                            params: cart_config,
                        },
                    )
                    .0;
            }
            _ => {
                unimplemented!("Mapper {}", header.mapper)
            }
        };

        let default_expansion_device = match header.version {
            INesVersion::V1 => None,
            INesVersion::V2 {
                default_expansion_device,
                ..
            } => default_expansion_device,
        }
        .unwrap_or(DefaultExpansionDevice::StandardControllers { swapped: false });

        let machine_builder = match default_expansion_device {
            DefaultExpansionDevice::StandardControllers { .. } => {
                let (machine_builder, _) = machine_builder.component(
                    "standard-nes-controllers",
                    NesControllerConfig { cpu_address_space },
                );

                machine_builder
            }
            DefaultExpansionDevice::FourScore => todo!(),
            DefaultExpansionDevice::SimpleFamiconFourPlayerAdaptor => todo!(),
            DefaultExpansionDevice::VsSystem { address: _ } => todo!(),
            DefaultExpansionDevice::VsZapper => todo!(),
            DefaultExpansionDevice::Zapper => todo!(),
            DefaultExpansionDevice::DualZapper => todo!(),
            DefaultExpansionDevice::BandaiHyperShotLightgun => todo!(),
            DefaultExpansionDevice::PowerPad { upside: _ } => todo!(),
            DefaultExpansionDevice::FamilyTrainer { upside: _ } => todo!(),
            DefaultExpansionDevice::ArkanoidVaus { kind: _ } => todo!(),
            DefaultExpansionDevice::DualArkanoidVausFamicomPlusDataRecorder => todo!(),
            DefaultExpansionDevice::KonamiHyperShotController => todo!(),
            DefaultExpansionDevice::CoconutsPachinkoController => todo!(),
            DefaultExpansionDevice::ExcitingBoxingPunchingBag => todo!(),
            DefaultExpansionDevice::JissenMahjongController => todo!(),
            DefaultExpansionDevice::PartyTap => todo!(),
            DefaultExpansionDevice::OekaKidsTablet => todo!(),
            DefaultExpansionDevice::SunsoftBarcodeBattler => todo!(),
            DefaultExpansionDevice::MiraclePianoKeyboard => todo!(),
            DefaultExpansionDevice::PokkunMoguraa => todo!(),
            DefaultExpansionDevice::TopRider => todo!(),
            DefaultExpansionDevice::DoubleFisted => todo!(),
            DefaultExpansionDevice::Famicom3dSystem => todo!(),
            DefaultExpansionDevice::ドレミッコKeyboard => todo!(),
            DefaultExpansionDevice::Rob { mode: _ } => todo!(),
            DefaultExpansionDevice::FamiconDataRecorder => todo!(),
            DefaultExpansionDevice::AsciiTurboFile => todo!(),
            DefaultExpansionDevice::IgsStorageBattleBox => todo!(),
            DefaultExpansionDevice::FamilyBasicKeyBoardPlusFamiconDataRecorder => todo!(),
            DefaultExpansionDevice::东达PECKeyboard => todo!(),
            DefaultExpansionDevice::普澤Bit79Keyboard => todo!(),
            DefaultExpansionDevice::小霸王Keyboard { mouse: _ } => todo!(),
            DefaultExpansionDevice::SnesMouse => todo!(),
            DefaultExpansionDevice::Multicart => todo!(),
            DefaultExpansionDevice::SnesControllers => todo!(),
            DefaultExpansionDevice::RacerMateBicycle => todo!(),
            DefaultExpansionDevice::UForce => todo!(),
            DefaultExpansionDevice::CityPatrolmanLightgun => todo!(),
            DefaultExpansionDevice::SharpC1CassetteInterface => todo!(),
            DefaultExpansionDevice::ExcaliburSudokuPad => todo!(),
            DefaultExpansionDevice::ABLPinball => todo!(),
            DefaultExpansionDevice::GoldenNuggetCasino => todo!(),
            DefaultExpansionDevice::科达Keyboard => todo!(),
            DefaultExpansionDevice::PortTestController => todo!(),
            DefaultExpansionDevice::BandaiMultiGamePlayerGamepad => todo!(),
            DefaultExpansionDevice::VenomTvDanceMat => todo!(),
            DefaultExpansionDevice::LgTvRemoteControl => todo!(),
            DefaultExpansionDevice::FamicomNetworkController => todo!(),
            DefaultExpansionDevice::KingFishingController => todo!(),
            DefaultExpansionDevice::CroakyKaraokeController => todo!(),
            DefaultExpansionDevice::科王Keyboard => todo!(),
            DefaultExpansionDevice::泽诚Keyboard => todo!(),
        };

        match header.timing_mode {
            // FIXME: Implementing Multi as NTSC for now
            TimingMode::Ntsc | TimingMode::Multi => {
                let (machine_builder, processor) = machine_builder.component(
                    "cpu",
                    fluxemu_definition_mos6502::Config::<Ricoh2A0x>::new(
                        Ntsc::master_clock() / 12,
                        cpu_address_space,
                    ),
                );

                let (machine_builder, _) = machine_builder.component(
                    "ppu",
                    PpuConfig::<Ntsc> {
                        ppu_address_space,
                        cpu_address_space,
                        processor,
                        _phantom: PhantomData,
                    },
                );

                let (machine_builder, _) =
                    machine_builder.component("apu", ApuConfig { cpu_address_space });

                machine_builder
            }
            TimingMode::Pal => {
                let (machine_builder, processor) = machine_builder.component(
                    "cpu",
                    fluxemu_definition_mos6502::Config::<Ricoh2A0x>::new(
                        Pal::master_clock() / 16,
                        cpu_address_space,
                    ),
                );

                let (machine_builder, _) = machine_builder.component(
                    "ppu",
                    PpuConfig::<Pal> {
                        ppu_address_space,
                        cpu_address_space,
                        processor,
                        _phantom: PhantomData,
                    },
                );

                let (machine_builder, _) =
                    machine_builder.component("apu", ApuConfig { cpu_address_space });

                machine_builder
            }
            TimingMode::Dendy => todo!(),
        }
        .seal()
    }
}

// Note that these are the *default* mapping for this particular cart
//
// The actual cart hardware is free to and often will immediately overwrite this
fn setup_ppu_nametables<P: Platform>(
    machine: MachineBuilder<P>,
    ppu_address_space: AddressSpaceId,
    nametables: &[ResourcePath; 2],
    ines: &INes,
) -> MachineBuilder<P> {
    match ines.mirroring {
        NametableMirroring::Vertical => machine.map_memory(
            ppu_address_space,
            [
                MemoryMapCommand::Map {
                    range: NAMETABLE_ADDRESSES[0].clone(),
                    permissions: Permissions::ALL,
                    target: MapTarget::Memory {
                        path: nametables[0].clone(),
                        subrange: None,
                    },
                },
                MemoryMapCommand::Map {
                    range: NAMETABLE_ADDRESSES[1].clone(),
                    permissions: Permissions::ALL,
                    target: MapTarget::Memory {
                        path: nametables[1].clone(),
                        subrange: None,
                    },
                },
                MemoryMapCommand::Map {
                    range: NAMETABLE_ADDRESSES[2].clone(),
                    permissions: Permissions::ALL,
                    target: MapTarget::Mirror {
                        destination: NAMETABLE_ADDRESSES[0].clone(),
                    },
                },
                MemoryMapCommand::Map {
                    range: NAMETABLE_ADDRESSES[3].clone(),
                    permissions: Permissions::ALL,
                    target: MapTarget::Mirror {
                        destination: NAMETABLE_ADDRESSES[1].clone(),
                    },
                },
            ],
        ),
        NametableMirroring::Horizontal => machine.map_memory(
            ppu_address_space,
            [
                MemoryMapCommand::Map {
                    range: NAMETABLE_ADDRESSES[0].clone(),
                    permissions: Permissions::ALL,
                    target: MapTarget::Memory {
                        path: nametables[0].clone(),
                        subrange: None,
                    },
                },
                MemoryMapCommand::Map {
                    range: NAMETABLE_ADDRESSES[2].clone(),
                    permissions: Permissions::ALL,
                    target: MapTarget::Memory {
                        path: nametables[1].clone(),
                        subrange: None,
                    },
                },
                MemoryMapCommand::Map {
                    range: NAMETABLE_ADDRESSES[1].clone(),
                    permissions: Permissions::ALL,
                    target: MapTarget::Mirror {
                        destination: NAMETABLE_ADDRESSES[0].clone(),
                    },
                },
                MemoryMapCommand::Map {
                    range: NAMETABLE_ADDRESSES[3].clone(),
                    permissions: Permissions::ALL,
                    target: MapTarget::Mirror {
                        destination: NAMETABLE_ADDRESSES[2].clone(),
                    },
                },
            ],
        ),
    }
}
