use std::{collections::HashMap, marker::PhantomData, ops::RangeInclusive};

use fluxemu_definition_mos6502::{Mos6502, Mos6502Event, Pin};
use fluxemu_range::ContiguousRange;
use fluxemu_runtime::{
    RuntimeApi,
    component::{
        Component,
        config::{ComponentConfig, LateContext, LateInitializedData},
    },
    event::{Event, EventMode, downcast_event},
    graphics::software::Texture,
    machine::builder::{ComponentBuilder, SchedulerParticipation},
    memory::{Address, AddressSpace, AddressSpaceId, MemoryError},
    path::{ComponentPath, ResourcePath},
    platform::Platform,
    scheduler::{Period, SynchronizationContext},
};
use nalgebra::Point2;
use serde::{Deserialize, Serialize};
use strum::FromRepr;

use crate::ppu::{
    backend::{PpuDisplayBackend, SupportedGraphicsApiPpu},
    background::{BackgroundPipelineState, BackgroundState, SpritePipelineState},
    color::{PPU_BLACK_INDEX, PpuColorIndex},
    oam::{OamState, SpriteEvaluationState},
    region::Region,
    state::{State, VramAddressPointerContents},
};

pub mod backend;
mod background;
mod color;
mod oam;
pub mod region;
mod state;
mod visible_scanlines;

#[derive(Clone, Copy, Debug, FromRepr)]
#[repr(u16)]
pub enum CpuAccessibleRegister {
    PpuCtrl = 0x2000,
    PpuMask = 0x2001,
    PpuStatus = 0x2002,
    OamAddr = 0x2003,
    OamData = 0x2004,
    PpuScroll = 0x2005,
    PpuAddr = 0x2006,
    PpuData = 0x2007,
    OamDma = 0x4014,
}

pub const NAMETABLE_ADDRESSES: [RangeInclusive<Address>; 4] = [
    0x2000..=0x23ff,
    0x2400..=0x27ff,
    0x2800..=0x2bff,
    0x2c00..=0x2fff,
];
pub const NAMETABLE_BASE_ADDRESS: Address = *NAMETABLE_ADDRESSES[0].start();
pub const BACKGROUND_PALETTE_BASE_ADDRESS: Address = 0x3f00;
pub const SPRITE_PALETTE_BASE_ADDRESS: Address = 0x3f10;
pub const ATTRIBUTE_BASE_ADDRESS: Address = NAMETABLE_BASE_ADDRESS + 0x3c0;
const DUMMY_SCANLINE_COUNT: u16 = 2;
const VISIBLE_SCANLINE_LENGTH: u16 = 256;
const HBLANK_LENGTH: u16 = 85;
const TOTAL_SCANLINE_LENGTH: u16 = VISIBLE_SCANLINE_LENGTH + HBLANK_LENGTH;
const INITIAL_CYCLE_COUNTER_POSITION: Point2<u16> = Point2::new(0, 0);

#[derive(Serialize, Deserialize, Debug, Clone, Copy)]
pub struct ColorEmphasis {
    /// This actually means green on pal/dendy
    pub red: bool,
    /// This actually means red on pal/dendy
    pub green: bool,
    pub blue: bool,
}

#[derive(Debug)]
pub struct PpuConfig<R: Region> {
    pub cpu_address_space: AddressSpaceId,
    pub ppu_address_space: AddressSpaceId,
    pub processor: ComponentPath,
    pub _phantom: PhantomData<R>,
}

#[derive(Debug)]
pub struct Ppu<R: Region, G: SupportedGraphicsApiPpu> {
    state: State,
    backend: Option<G::Backend<R>>,
    cpu_address_space: AddressSpaceId,
    ppu_address_space: AddressSpaceId,
    framebuffer_path: ResourcePath,
    processor_path: ComponentPath,
    staging_buffer: Texture<PpuColorIndex>,
    timestamp: Period,
    period: Period,
}

impl<R: Region, P: Platform<GraphicsApi: SupportedGraphicsApiPpu>> ComponentConfig<P>
    for PpuConfig<R>
{
    type Component = Ppu<R, P::GraphicsApi>;

    fn late_initialize(
        component: &mut Self::Component,
        data: &LateContext<P>,
    ) -> LateInitializedData<P> {
        let backend = <P::GraphicsApi as SupportedGraphicsApiPpu>::Backend::new(
            data.graphics_initialization_data.clone(),
        );
        let framebuffer = backend.create_framebuffer();
        component.backend = Some(backend);

        let framebuffer_name = component.framebuffer_path.name().to_string().into();

        LateInitializedData {
            framebuffers: HashMap::from_iter([(framebuffer_name, framebuffer)]),
        }
    }

    fn build_component(
        self,
        component_builder: ComponentBuilder<'_, '_, P, Self::Component>,
    ) -> Result<Self::Component, Box<dyn std::error::Error>> {
        let frequency = R::master_clock() / 4;

        let (component_builder, framebuffer_path) = component_builder
            .scheduler_participation(Some(SchedulerParticipation::OnAccess))
            .framebuffer("framebuffer");

        let total_screen_time =
            Period::from_num(TOTAL_SCANLINE_LENGTH as u32 * R::TOTAL_SCANLINES as u32) / frequency;
        let framerate = total_screen_time.recip();

        let vblank_start_from_initial_position =
            (Period::from_num(TOTAL_SCANLINE_LENGTH) * 241 + Period::from_num(1)) / frequency;

        let vblank_end_from_initial_position =
            (Period::from_num(TOTAL_SCANLINE_LENGTH) * 261 + Period::from_num(1)) / frequency;

        let my_path = component_builder.path().clone();

        component_builder
            .memory_map_component_write(
                self.cpu_address_space,
                CpuAccessibleRegister::PpuCtrl as usize..=CpuAccessibleRegister::PpuCtrl as usize,
            )
            .memory_map_component_write(
                self.cpu_address_space,
                CpuAccessibleRegister::PpuScroll as usize
                    ..=CpuAccessibleRegister::PpuScroll as usize,
            )
            .memory_map_component_write(
                self.cpu_address_space,
                CpuAccessibleRegister::PpuMask as usize..=CpuAccessibleRegister::PpuMask as usize,
            )
            .memory_map_component_read(
                self.cpu_address_space,
                CpuAccessibleRegister::PpuStatus as usize
                    ..=CpuAccessibleRegister::PpuStatus as usize,
            )
            .memory_map_component(
                self.cpu_address_space,
                CpuAccessibleRegister::PpuAddr as usize..=CpuAccessibleRegister::PpuAddr as usize,
            )
            .memory_map_component(
                self.cpu_address_space,
                CpuAccessibleRegister::PpuData as usize..=CpuAccessibleRegister::PpuData as usize,
            )
            .memory_map_component(
                self.cpu_address_space,
                CpuAccessibleRegister::OamAddr as usize..=CpuAccessibleRegister::OamAddr as usize,
            )
            .memory_map_component(
                self.cpu_address_space,
                CpuAccessibleRegister::OamData as usize..=CpuAccessibleRegister::OamData as usize,
            )
            .memory_map_component_write(
                self.cpu_address_space,
                CpuAccessibleRegister::OamDma as usize..=CpuAccessibleRegister::OamDma as usize,
            )
            .schedule_event::<Self::Component>(
                // x: 1, y: 241
                &my_path,
                vblank_start_from_initial_position,
                EventMode::Repeating {
                    frequency: framerate,
                },
                PpuEvent::VblankStart,
            )
            .schedule_event::<Self::Component>(
                // x: 1, y: 261
                &my_path,
                vblank_end_from_initial_position,
                EventMode::Repeating {
                    frequency: framerate,
                },
                PpuEvent::VblankEnd,
            );

        let staging_buffer = Texture::new(
            VISIBLE_SCANLINE_LENGTH as usize,
            R::VISIBLE_SCANLINES as usize,
            PPU_BLACK_INDEX,
        );

        Ok(Ppu {
            state: State {
                vblank_nmi_enabled: false,
                greyscale: false,
                entered_vblank: false,
                vram_address_pointer_write_phase: false,
                vram_address_pointer_increment_amount: 1,
                vram_read_buffer: 0,
                color_emphasis: ColorEmphasis {
                    red: false,
                    green: false,
                    blue: false,
                },
                cycle_counter: INITIAL_CYCLE_COUNTER_POSITION,
                background_pipeline_state: BackgroundPipelineState::FetchingNametable,
                sprite_pipeline_state: SpritePipelineState::FetchingNametableGarbage0,
                oam: OamState {
                    data: rand::random(),
                    oam_addr: 0x00,
                    sprite_evaluation_state: SpriteEvaluationState::InspectingY,
                    secondary_data: heapless::Vec::new(),
                    sprite_zero_in_secondary: false,
                    currently_rendering_sprites: heapless::Vec::new(),
                    show_leftmost_pixels: true,
                    sprite_8x8_pattern_table_index: 0x0000,
                    rendering_enabled: false,
                    awaiting_memory_access: true,
                    sprite_zero_hit: false,
                    sprite_8x16_mode: false,
                },
                background: BackgroundState {
                    pattern_table_index: 0x0000,
                    pattern_low_shift: 0,
                    pattern_high_shift: 0,
                    attribute_shift: 0,
                    fine_x_scroll: 0,
                    rendering_enabled: false,
                    awaiting_memory_access: true,
                    tile_pixel: 0,
                    show_leftmost_pixels: true,
                },
                vram_address_pointer: 0,
                shadow_vram_address_pointer: 0,
            },
            backend: None,
            staging_buffer,
            cpu_address_space: self.cpu_address_space,
            processor_path: self.processor.clone(),
            ppu_address_space: self.ppu_address_space,
            framebuffer_path,
            timestamp: Period::default(),
            period: frequency.recip(),
        })
    }
}

impl<R: Region, G: SupportedGraphicsApiPpu> Component for Ppu<R, G> {
    type Event = PpuEvent;

    fn load_snapshot(
        &mut self,
        _version: fluxemu_runtime::component::ComponentVersion,
        _reader: &mut dyn std::io::Read,
    ) -> Result<(), Box<dyn std::error::Error>> {
        todo!()
    }

    fn store_snapshot(
        &self,
        _writer: &mut dyn std::io::Write,
    ) -> Result<(), Box<dyn std::error::Error>> {
        todo!()
    }

    fn memory_read(
        &mut self,
        address: Address,
        _address_space: AddressSpaceId,
        avoid_side_effects: bool,
        buffer: &mut [u8],
    ) -> Result<(), MemoryError> {
        for (address, buffer) in
            RangeInclusive::from_start_and_length(address, buffer.len()).zip(buffer.iter_mut())
        {
            let register = CpuAccessibleRegister::from_repr(address as u16).unwrap();
            tracing::trace!("Reading from PPU register: {:?}", register);

            match register {
                CpuAccessibleRegister::PpuMask => todo!(),
                CpuAccessibleRegister::PpuStatus => {
                    if !avoid_side_effects {
                        self.state.vram_address_pointer_write_phase = false;
                    }

                    let vblank = if avoid_side_effects {
                        self.state.entered_vblank
                    } else {
                        std::mem::take(&mut self.state.entered_vblank)
                    };

                    *buffer = (*buffer & 0b0011_1111)
                        | ((vblank as u8) << 7)
                        | ((self.state.oam.sprite_zero_hit as u8) << 6);
                }
                CpuAccessibleRegister::OamAddr => {
                    *buffer = self.state.oam.oam_addr;
                }
                CpuAccessibleRegister::OamData => {
                    *buffer = self.state.oam.data[self.state.oam.oam_addr as usize];
                }
                CpuAccessibleRegister::PpuScroll => todo!(),
                CpuAccessibleRegister::PpuAddr => todo!(),
                CpuAccessibleRegister::PpuData => {
                    let runtime = RuntimeApi::current();
                    let mut ppu_address_space =
                        runtime.address_space(self.ppu_address_space).unwrap();

                    if avoid_side_effects {
                        *buffer = ppu_address_space.read_le_value_pure(
                            self.state.vram_address_pointer as usize,
                            self.timestamp,
                        )?;
                    } else {
                        let new_value = ppu_address_space.read_le_value::<u8>(
                            self.state.vram_address_pointer as usize,
                            self.timestamp,
                        )?;

                        *buffer = std::mem::replace(&mut self.state.vram_read_buffer, new_value);
                    }
                }
                _ => {
                    unreachable!("{:?}", register);
                }
            }
        }

        Ok(())
    }

    fn memory_write(
        &mut self,
        address: Address,
        _address_space: AddressSpaceId,
        buffer: &[u8],
    ) -> Result<(), MemoryError> {
        for (address, buffer) in
            RangeInclusive::from_start_and_length(address, buffer.len()).zip(buffer.iter())
        {
            let register = CpuAccessibleRegister::from_repr(address as u16).unwrap();
            tracing::trace!("Writing to PPU register: {:?}", register);

            match register {
                CpuAccessibleRegister::PpuCtrl => {
                    let mut shadow_vram_address_pointer =
                        VramAddressPointerContents::from(self.state.shadow_vram_address_pointer);

                    shadow_vram_address_pointer.nametable.x = *buffer & 0b0000_0001 != 0;
                    shadow_vram_address_pointer.nametable.y = *buffer & 0b0000_0010 != 0;

                    self.state.vram_address_pointer_increment_amount =
                        if *buffer & 0b0000_0100 != 0 { 32 } else { 1 };

                    self.state.oam.sprite_8x8_pattern_table_index =
                        (*buffer & 0b0000_1000 != 0) as u8;

                    self.state.background.pattern_table_index = (*buffer & 0b0001_0000 != 0) as u8;

                    self.state.vblank_nmi_enabled = *buffer & 0b1000_0000 != 0;
                    self.state.shadow_vram_address_pointer = shadow_vram_address_pointer.into();
                }
                CpuAccessibleRegister::PpuMask => {
                    self.state.greyscale = *buffer & 0b0000_0001 != 0;
                    self.state.background.show_leftmost_pixels = *buffer & 0b0000_0010 != 0;
                    self.state.oam.show_leftmost_pixels = *buffer & 0b0000_0100 != 0;
                    self.state.background.rendering_enabled = *buffer & 0b0000_1000 != 0;
                    self.state.oam.rendering_enabled = *buffer & 0b0001_0000 != 0;
                    self.state.color_emphasis.red = *buffer & 0b0010_0000 != 0;
                    self.state.color_emphasis.green = *buffer & 0b0100_0000 != 0;
                    self.state.color_emphasis.blue = *buffer & 0b1000_0000 != 0;
                }
                CpuAccessibleRegister::OamAddr => {
                    self.state.oam.oam_addr = *buffer;
                }
                CpuAccessibleRegister::OamData => {
                    self.state.oam.data[self.state.oam.oam_addr as usize] = *buffer;
                    self.state.oam.oam_addr = self.state.oam.oam_addr.wrapping_add(1);
                }
                CpuAccessibleRegister::PpuScroll => {
                    let mut shadow_vram_address_pointer =
                        VramAddressPointerContents::from(self.state.shadow_vram_address_pointer);

                    if self.state.vram_address_pointer_write_phase {
                        // fine scroll y
                        shadow_vram_address_pointer.fine_y = *buffer & 0b0000_0111;
                        // coarse scroll y
                        shadow_vram_address_pointer.coarse.y = (*buffer & 0b1111_1000) >> 3;
                    } else {
                        // fine scroll x
                        self.state.background.fine_x_scroll = *buffer & 0b0000_0111;
                        // coarse scroll x
                        shadow_vram_address_pointer.coarse.x = (*buffer & 0b1111_1000) >> 3;
                    }

                    self.state.vram_address_pointer_write_phase =
                        !self.state.vram_address_pointer_write_phase;
                    self.state.shadow_vram_address_pointer = shadow_vram_address_pointer.into();
                }
                CpuAccessibleRegister::PpuAddr => {
                    let mut unpacked_address = self.state.shadow_vram_address_pointer.to_be_bytes();

                    unpacked_address[usize::from(self.state.vram_address_pointer_write_phase)] =
                        *buffer;
                    self.state.shadow_vram_address_pointer =
                        u16::from_be_bytes(unpacked_address) & 0b0111_1111_1111_1111;

                    // Write the completed address
                    if self.state.vram_address_pointer_write_phase {
                        self.state.vram_address_pointer = self.state.shadow_vram_address_pointer;
                    }

                    self.state.vram_address_pointer_write_phase =
                        !self.state.vram_address_pointer_write_phase;
                }
                CpuAccessibleRegister::PpuData => {
                    let runtime = RuntimeApi::current();
                    let mut ppu_address_space =
                        runtime.address_space(self.ppu_address_space).unwrap();

                    // Redirect into the ppu address space
                    ppu_address_space.write_le_value(
                        self.state.vram_address_pointer as usize,
                        self.timestamp,
                        *buffer,
                    )?;

                    self.state.vram_address_pointer =
                        self.state.vram_address_pointer.wrapping_add(u16::from(
                            self.state.vram_address_pointer_increment_amount,
                        )) & 0b0111_1111_1111_1111;
                }
                CpuAccessibleRegister::OamDma => {
                    let runtime = RuntimeApi::current();
                    let mut cpu_address_space =
                        runtime.address_space(self.cpu_address_space).unwrap();

                    let page = u16::from(*buffer) << 8;

                    runtime.schedule_event::<Mos6502>(
                        &self.processor_path,
                        EventMode::Once,
                        self.timestamp,
                        Mos6502Event::FlagChange {
                            pin: Pin::Rdy,
                            value: false,
                        },
                    );

                    // TODO: Extract to constant or extract from cpu directly within the config builder
                    let processor_frequency = R::master_clock() / 12;

                    let next_processor_rdy_high =
                        self.timestamp + (processor_frequency.recip() * 514);

                    // Make sure the cpu wakes up
                    runtime.schedule_event::<Mos6502>(
                        &self.processor_path,
                        EventMode::Once,
                        next_processor_rdy_high,
                        Mos6502Event::FlagChange {
                            pin: Pin::Rdy,
                            value: true,
                        },
                    );

                    // Read off OAM data immediately, this is done for performance and should not
                    // have any side effects
                    let _ = cpu_address_space.read(
                        page as usize,
                        self.timestamp,
                        &mut self.state.oam.data,
                    );
                }
                _ => {
                    unreachable!("{:?}", register);
                }
            }
        }

        Ok(())
    }

    fn handle_event(&mut self, event: Box<dyn Event>) {
        let runtime = RuntimeApi::current();
        let event = downcast_event::<Self>(event);

        match event {
            PpuEvent::VblankStart => {
                self.state.entered_vblank = true;

                if self.state.vblank_nmi_enabled {
                    runtime.schedule_event::<Mos6502>(
                        &self.processor_path,
                        EventMode::Once,
                        self.timestamp,
                        Mos6502Event::FlagChange {
                            pin: Pin::Nmi,
                            value: false,
                        },
                    );
                }
            }
            PpuEvent::VblankEnd => {
                self.state.entered_vblank = false;

                runtime.schedule_event::<Mos6502>(
                    &self.processor_path,
                    EventMode::Once,
                    self.timestamp,
                    Mos6502Event::FlagChange {
                        pin: Pin::Nmi,
                        value: true,
                    },
                );

                runtime.commit_framebuffer::<G>(&self.framebuffer_path, |framebuffer| {
                    self.backend
                        .as_mut()
                        .unwrap()
                        .commit_staging_buffer(&self.staging_buffer, framebuffer);
                });
            }
        }
    }

    fn synchronize(&mut self, mut context: SynchronizationContext) {
        let runtime = RuntimeApi::current();
        let mut ppu_address_space = runtime.address_space(self.ppu_address_space).unwrap();

        for now in context.allocate(self.period) {
            self.timestamp = now;

            if (0..R::VISIBLE_SCANLINES).contains(&self.state.cycle_counter.y) {
                self.handle_visible_scanlines(&mut ppu_address_space);
            } else if self.state.cycle_counter.y == 261 {
                self.handle_prerender(&mut ppu_address_space);
            }

            self.state.cycle_counter.x += 1;

            if self.state.cycle_counter.x >= TOTAL_SCANLINE_LENGTH {
                self.state.cycle_counter.x = 0;
                self.state.cycle_counter.y += 1;
            }

            if self.state.cycle_counter.y >= R::TOTAL_SCANLINES {
                self.state.cycle_counter.y = 0;
            }
        }
    }

    fn needs_work(&self, delta: Period) -> bool {
        delta >= self.period
    }
}

impl<R: Region, G: SupportedGraphicsApiPpu> Ppu<R, G> {
    fn handle_prerender(&mut self, ppu_address_space: &mut AddressSpace<'_>) {
        if self.state.cycle_counter.x == 1 {
            self.state.oam.sprite_zero_hit = false;
        }

        if self.state.cycle_counter.x == 257
            && (self.state.background.rendering_enabled || self.state.oam.rendering_enabled)
        {
            let t = VramAddressPointerContents::from(self.state.shadow_vram_address_pointer);
            let mut v = VramAddressPointerContents::from(self.state.vram_address_pointer);

            v.nametable.x = t.nametable.x;
            v.coarse.x = t.coarse.x;

            self.state.vram_address_pointer = u16::from(v);
        }

        if let 280..=304 = self.state.cycle_counter.x
            && (self.state.background.rendering_enabled || self.state.oam.rendering_enabled)
        {
            let t = VramAddressPointerContents::from(self.state.shadow_vram_address_pointer);
            let mut v = VramAddressPointerContents::from(self.state.vram_address_pointer);

            v.nametable.y = t.nametable.y;
            v.coarse.y = t.coarse.y;
            v.fine_y = t.fine_y;

            self.state.vram_address_pointer = u16::from(v);
        }

        if let 305..=320 = self.state.cycle_counter.x
            && self.state.background.rendering_enabled
        {
            self.state
                .drive_background_pipeline::<R>(ppu_address_space, self.timestamp);
        }
    }
}

#[derive(Debug, Clone)]
pub enum PpuEvent {
    VblankStart,
    VblankEnd,
}
