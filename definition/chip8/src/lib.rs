use std::{marker::PhantomData, ops::RangeInclusive};

use audio::Chip8AudioConfig;
use bytes::Bytes;
use display::Chip8DisplayConfig;
use fluxemu_runtime::{
    machine::builder::{MachineBuilder, MachineFactory, RomRequirement},
    memory::{MapTarget, MemoryMapCommand, Permissions},
    platform::Platform,
    scheduler::Frequency,
};
use font::CHIP8_FONT;
use processor::Chip8ProcessorConfig;
use serde::{Deserialize, Serialize};
use timer::Chip8TimerConfig;

use fluxemu_range::ContiguousRange;

use crate::display::SupportedGraphicsApiChip8Display;

mod audio;
mod display;
mod font;
mod processor;
mod timer;

#[derive(Serialize, Deserialize, PartialEq, Eq, Clone, Copy, Debug, Default)]
pub enum Chip8Mode {
    #[default]
    Chip8,
    Chip8x,
    Chip48,
    SuperChip8,
    XoChip,
}

#[derive(Debug, Default)]
pub struct Chip8;

impl<P: Platform<GraphicsApi: SupportedGraphicsApiChip8Display>> MachineFactory<P> for Chip8 {
    fn construct<'a>(&self, machine: MachineBuilder<P>) -> MachineBuilder<P> {
        let (machine, cpu_address_space) = machine.address_space(12);
        let (machine, timer) = machine.default_component::<Chip8TimerConfig>("timer");
        let (machine, audio) = machine.component(
            "audio",
            Chip8AudioConfig {
                processor_frequency: Frequency::from_num(600),
            },
        );
        let (machine, display) = machine.default_component::<Chip8DisplayConfig>("display");
        let (machine, _) = machine.component(
            "cpu",
            Chip8ProcessorConfig {
                cpu_address_space,
                timer,
                audio,
                display,
                frequency: Frequency::from_num(700),
                force_mode: None,
                always_shr_in_place: false,
                stall_on_draw_until_vsync: false,
                _phantom: PhantomData,
            },
        );

        let program_specification = machine.program_specification().unwrap();
        let filesystem = program_specification.info.filesystem();

        assert_eq!(
            filesystem.len(),
            1,
            "CHIP8 programs only contain a single ROM"
        );

        let rom_id = filesystem
            .first_key_value()
            .map(|(rom_id, _)| rom_id)
            .copied()
            .unwrap();

        let rom = machine
            .open_rom(rom_id, RomRequirement::Required)
            .unwrap()
            .unwrap();

        let chip8_font = bytemuck::cast_slice(&CHIP8_FONT);

        let (machine, ram_path) = machine.memory(
            "ram",
            0x1000,
            [
                (
                    RangeInclusive::from_start_and_length(0, chip8_font.len()),
                    Bytes::from_static(chip8_font),
                ),
                (RangeInclusive::from_start_and_length(0x200, rom.len()), rom),
            ],
        );

        machine.map_memory(
            cpu_address_space,
            [MemoryMapCommand::Map {
                range: 0x000..=0xfff,
                permissions: Permissions::ALL,
                target: MapTarget::Memory {
                    path: ram_path,
                    subrange: None,
                },
            }],
        )
    }
}
