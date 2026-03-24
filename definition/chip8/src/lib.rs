use std::{borrow::Cow, marker::PhantomData};

use audio::Chip8AudioConfig;
use display::Chip8DisplayConfig;
use fluxemu_definition_memory::{InitialContents, MemoryConfig};
use fluxemu_runtime::{
    machine::builder::{MachineBuilder, MachineFactory},
    platform::Platform,
    scheduler::Frequency,
};
use font::CHIP8_FONT;
use processor::Chip8ProcessorConfig;
use rangemap::RangeInclusiveMap;
use serde::{Deserialize, Serialize};
use timer::Chip8TimerConfig;

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
    fn construct<'a>(&self, machine: MachineBuilder<'a, P>) -> MachineBuilder<'a, P> {
        let (machine, cpu_address_space) = machine.address_space(12);
        let (machine, timer) = machine.default_component::<Chip8TimerConfig>("timer");
        let (machine, audio) = machine.component(
            "audio",
            Chip8AudioConfig {
                processor_frequency: Frequency::from_num(1000),
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
                frequency: Frequency::from_num(1000),
                force_mode: None,
                always_shr_in_place: false,
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

        let (machine, _) = machine.component(
            "workram",
            MemoryConfig {
                readable: true,
                writable: true,
                assigned_range: 0x000..=0xfff,
                assigned_address_space: cpu_address_space,
                initial_contents: RangeInclusiveMap::from_iter([
                    (
                        0x000..=0x04f,
                        InitialContents::Array(Cow::Borrowed(bytemuck::cast_slice(&CHIP8_FONT))),
                    ),
                    (0x200..=0xfff, InitialContents::Rom(rom_id)),
                ]),
                sram: false,
            },
        );

        machine
    }
}
