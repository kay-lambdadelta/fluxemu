use std::marker::PhantomData;

pub use cartridge::ines::INes;
use cartridge::{CartParams, ines::TimingMode};
use fluxemu_definition_mos6502::{Mos6502Config, Mos6502Kind};
use fluxemu_runtime::{
    ComponentPath,
    machine::builder::{MachineBuilder, MachineFactory, RomRequirement},
    memory::{
        AddressSpaceId,
        component::{InitialContents, MemoryConfig},
    },
    platform::Platform,
};
use ppu::PpuConfig;
use rangemap::RangeInclusiveMap;

use crate::{
    apu::ApuConfig,
    cartridge::{
        ines::{INesVersion, NametableMirroring, expansion_device::DefaultExpansionDevice},
        mapper::{mmc1::Mmc1Config, nrom::NRomConfig},
    },
    gamepad::controller::NesControllerConfig,
    ppu::{
        BACKGROUND_PALETTE_BASE_ADDRESS, NAMETABLE_ADDRESSES,
        backend::SupportedGraphicsApiPpu,
        region::{Region, ntsc::Ntsc},
    },
};

mod apu;
mod cartridge;
mod gamepad;
mod ppu;

#[derive(Debug, Default)]
pub struct Nes;

impl<G: SupportedGraphicsApiPpu, P: Platform<GraphicsApi = G>> MachineFactory<P> for Nes {
    fn construct<'a>(&self, machine: MachineBuilder<'a, P>) -> MachineBuilder<'a, P> {
        let (machine, cpu_address_space) = machine.address_space(16);
        let (machine, ppu_address_space) = machine.address_space(14);

        let program_specification = machine.program_specification().unwrap();
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

        let mut machine = machine
            .component(
                "work-ram",
                MemoryConfig {
                    readable: true,
                    writable: true,
                    assigned_range: 0x0000..=0x07ff,
                    assigned_address_space: cpu_address_space,
                    initial_contents: RangeInclusiveMap::from_iter([(
                        0x0000..=0x07ff,
                        InitialContents::Random,
                    )]),
                    sram: false,
                },
            )
            .0
            .component(
                "palette-ram",
                MemoryConfig {
                    readable: true,
                    writable: true,
                    assigned_range: BACKGROUND_PALETTE_BASE_ADDRESS
                        ..=BACKGROUND_PALETTE_BASE_ADDRESS + 0x1f,
                    assigned_address_space: ppu_address_space,
                    initial_contents: RangeInclusiveMap::from_iter([(
                        BACKGROUND_PALETTE_BASE_ADDRESS..=BACKGROUND_PALETTE_BASE_ADDRESS + 0x1f,
                        InitialContents::Random,
                    )]),
                    sram: false,
                },
            )
            .0
            .memory_map_mirror(cpu_address_space, 0x0800..=0x0fff, 0x0000..=0x07ff)
            .memory_map_mirror(cpu_address_space, 0x1000..=0x17ff, 0x0000..=0x07ff)
            .memory_map_mirror(cpu_address_space, 0x1800..=0x1fff, 0x0000..=0x07ff)
            .memory_map_mirror(ppu_address_space, 0x3f10..=0x3f10, 0x3f00..=0x3f00)
            .memory_map_mirror(ppu_address_space, 0x3f14..=0x3f14, 0x3f04..=0x3f04)
            .memory_map_mirror(ppu_address_space, 0x3f18..=0x3f18, 0x3f08..=0x3f08)
            .memory_map_mirror(ppu_address_space, 0x3f1c..=0x3f1c, 0x3f0c..=0x3f0c);

        for address in (0x2000..=0x3fff).step_by(8).skip(1) {
            machine = machine.memory_map_mirror(
                cpu_address_space,
                address..=address + 7,
                0x2000..=0x2007,
            );
        }

        let rom = machine
            .open_rom(rom_id, RomRequirement::Required)
            .unwrap()
            .unwrap();

        let header = INes::parse(rom[0..16].try_into().unwrap()).unwrap();
        if header.trainer {
            tracing::warn!("This ROM contains a trainer, which is not emulated at this time");
        }
        let (mut machine, nametables) = setup_ppu_nametables(machine, ppu_address_space, &header);

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
                machine = machine
                    .component(
                        "nrom_cartridge",
                        NRomConfig {
                            config: cart_config,
                        },
                    )
                    .0;
            }
            001 | 155 => {
                machine = machine
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

        let machine = match default_expansion_device {
            DefaultExpansionDevice::StandardControllers { .. } => {
                let (machine, _) = machine.component(
                    "standard-nes-controller-0",
                    NesControllerConfig {
                        cpu_address_space,
                        controller_index: 0,
                    },
                );

                /*
                let (machine, _) = machine.insert_component(
                    "standard-nes-controller-1",
                    NesControllerConfig {
                        cpu_address_space,
                        controller_index: 1,
                    },
                );
                */

                machine
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
            DefaultExpansionDevice::FamiconDataRecorder => machine,
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
                let processor_frequency = Ntsc::master_clock() / 12;

                let (machine, processor) = machine.component(
                    "cpu",
                    Mos6502Config {
                        frequency: processor_frequency,
                        assigned_address_space: cpu_address_space,
                        kind: Mos6502Kind::Ricoh2A0x,
                        broken_ror: false,
                    },
                );

                let (machine, _) = machine.component(
                    "ppu",
                    PpuConfig::<Ntsc> {
                        ppu_address_space,
                        cpu_address_space,
                        processor,
                        _phantom: PhantomData,
                    },
                );

                let (machine, _) = machine.component("apu", ApuConfig { cpu_address_space });

                machine
            }
            TimingMode::Pal => todo!(),
            TimingMode::Dendy => todo!(),
        }
    }
}

// Note that these are the *default* mapping for this particular cart
//
// The actual cart hardware is free to and often will immediately overwrite this
fn setup_ppu_nametables<'a, P: Platform>(
    machine: MachineBuilder<'a, P>,
    ppu_address_space: AddressSpaceId,
    ines: &INes,
) -> (MachineBuilder<'a, P>, [ComponentPath; 2]) {
    match ines.mirroring {
        NametableMirroring::Vertical => {
            let (machine, nametable_0) = machine.component(
                "nametable-0",
                MemoryConfig {
                    assigned_address_space: ppu_address_space,
                    assigned_range: NAMETABLE_ADDRESSES[0].clone(),
                    readable: true,
                    writable: true,
                    initial_contents: RangeInclusiveMap::from_iter([(
                        NAMETABLE_ADDRESSES[0].clone(),
                        InitialContents::Random,
                    )]),
                    sram: false,
                },
            );

            let (machine, nametable_1) = machine.component(
                "nametable-1",
                MemoryConfig {
                    assigned_address_space: ppu_address_space,
                    assigned_range: NAMETABLE_ADDRESSES[1].clone(),
                    readable: true,
                    writable: true,
                    initial_contents: RangeInclusiveMap::from_iter([(
                        NAMETABLE_ADDRESSES[1].clone(),
                        InitialContents::Random,
                    )]),
                    sram: false,
                },
            );

            let machine = machine
                .memory_map_mirror(
                    ppu_address_space,
                    NAMETABLE_ADDRESSES[2].clone(),
                    NAMETABLE_ADDRESSES[0].clone(),
                )
                .memory_map_mirror(
                    ppu_address_space,
                    NAMETABLE_ADDRESSES[3].clone(),
                    NAMETABLE_ADDRESSES[1].clone(),
                );

            (machine, [nametable_0, nametable_1])
        }
        NametableMirroring::Horizontal => {
            let (machine, nametable_0) = machine.component(
                "nametable-0",
                MemoryConfig {
                    assigned_address_space: ppu_address_space,
                    assigned_range: NAMETABLE_ADDRESSES[0].clone(),
                    readable: true,
                    writable: true,
                    initial_contents: RangeInclusiveMap::from_iter([(
                        NAMETABLE_ADDRESSES[0].clone(),
                        InitialContents::Random,
                    )]),
                    sram: false,
                },
            );

            let (machine, nametable_1) = machine.component(
                "nametable-1",
                MemoryConfig {
                    assigned_address_space: ppu_address_space,
                    assigned_range: NAMETABLE_ADDRESSES[2].clone(),
                    readable: true,
                    writable: true,
                    initial_contents: RangeInclusiveMap::from_iter([(
                        NAMETABLE_ADDRESSES[2].clone(),
                        InitialContents::Random,
                    )]),
                    sram: false,
                },
            );

            let machine = machine
                .memory_map_mirror(
                    ppu_address_space,
                    NAMETABLE_ADDRESSES[1].clone(),
                    NAMETABLE_ADDRESSES[0].clone(),
                )
                .memory_map_mirror(
                    ppu_address_space,
                    NAMETABLE_ADDRESSES[3].clone(),
                    NAMETABLE_ADDRESSES[2].clone(),
                );

            (machine, [nametable_0, nametable_1])
        }
    }
}
