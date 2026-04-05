use std::{
    collections::HashMap,
    marker::PhantomData,
    ops::RangeInclusive,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU8, Ordering},
    },
};

use fluxemu_definition_mos6502::{Mos6502, NmiFlag, RdyFlag};
use fluxemu_range::ContiguousRange;
use fluxemu_runtime::{
    RuntimeApi,
    component::{
        Component,
        config::{ComponentConfig, LateContext, LateInitializedData},
    },
    event::{EventRequeueMode, EventType},
    graphics::software::Texture,
    machine::builder::{ComponentBuilder, SchedulerParticipation},
    memory::{Address, AddressSpaceCache, AddressSpaceId, MemoryError},
    path::{ComponentPath, ResourcePath},
    platform::Platform,
    scheduler::{Period, SynchronizationContext},
};
use nalgebra::{Point2, Vector2};
use serde::{Deserialize, Serialize};
use strum::FromRepr;

use crate::ppu::{
    backend::{PpuDisplayBackend, SupportedGraphicsApiPpu},
    background::{BackgroundPipelineState, BackgroundState, SpritePipelineState},
    color::{PPU_BLACK_INDEX, PpuColorIndex},
    oam::{OamSprite, OamState, SpriteEvaluationState},
    region::Region,
    state::{State, VramAddressPointerContents},
};

pub mod backend;
mod background;
mod color;
mod oam;
pub mod region;
mod state;

const VBLANK_START: &str = "vblank_start";
const VBLANK_END: &str = "vblank_end";
const WAKEUP_CPU_VIA_RDY: &str = "wakeup_cpu_via_rdy";

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
    processor_rdy: Option<Arc<RdyFlag>>,
    processor_nmi: Option<Arc<NmiFlag>>,
    ppu_address_space_cache: Option<AddressSpaceCache>,
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
        let runtime = RuntimeApi::current();

        let processor_nmi = runtime
            .registry()
            .interact::<Mos6502, _>(&component.processor_path, Period::ZERO, Mos6502::nmi)
            .unwrap();

        let processor_rdy = runtime
            .registry()
            .interact::<Mos6502, _>(&component.processor_path, Period::ZERO, Mos6502::rdy)
            .unwrap();

        component.processor_nmi = Some(processor_nmi);
        component.processor_rdy = Some(processor_rdy);

        component.ppu_address_space_cache = Some(
            runtime
                .address_space(component.ppu_address_space)
                .unwrap()
                .create_cache(),
        );

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
            .insert_event(
                // x: 1, y: 241
                &my_path,
                VBLANK_START,
                vblank_start_from_initial_position,
                EventRequeueMode::Repeating {
                    frequency: framerate,
                },
                EventType::sync_point(),
            )
            .insert_event(
                // x: 1, y: 261
                &my_path,
                VBLANK_END,
                vblank_end_from_initial_position,
                EventRequeueMode::Repeating {
                    frequency: framerate,
                },
                EventType::sync_point(),
            );

        let staging_buffer = Texture::new(
            VISIBLE_SCANLINE_LENGTH as usize,
            R::VISIBLE_SCANLINES as usize,
            PPU_BLACK_INDEX,
        );

        Ok(Ppu {
            state: State {
                sprite_size: Vector2::new(8, 8),
                vblank_nmi_enabled: false,
                greyscale: false,
                entered_vblank: AtomicBool::new(false),
                show_background_leftmost_pixels: false,
                vram_address_pointer_write_phase: false,
                vram_address_pointer_increment_amount: 1,
                vram_read_buffer: AtomicU8::new(0),
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
                    currently_rendering_sprites: heapless::Vec::new(),
                    show_sprites_leftmost_pixels: true,
                    sprite_8x8_pattern_table_index: 0x0000,
                    rendering_enabled: false,
                    awaiting_memory_access: true,
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
                },
                vram_address_pointer: 0,
                shadow_vram_address_pointer: 0,
            },
            backend: None,
            staging_buffer,
            cpu_address_space: self.cpu_address_space,
            processor_nmi: None,
            processor_rdy: None,
            processor_path: self.processor.clone(),
            ppu_address_space: self.ppu_address_space,
            ppu_address_space_cache: None,
            framebuffer_path,
            timestamp: Period::default(),
            period: frequency.recip(),
        })
    }
}

impl<R: Region, G: SupportedGraphicsApiPpu> Component for Ppu<R, G> {
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
        &self,
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
                    let bit = if avoid_side_effects {
                        self.state.entered_vblank.load(Ordering::Acquire)
                    } else {
                        self.state.entered_vblank.swap(false, Ordering::AcqRel)
                    };

                    *buffer = (*buffer & 0b0111_1111) | (bit as u8) << 7;
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
                    let ppu_address_space = runtime.address_space(self.ppu_address_space).unwrap();

                    if avoid_side_effects {
                        *buffer = ppu_address_space.read_le_value_pure(
                            self.state.vram_address_pointer as usize,
                            self.timestamp,
                            None,
                        )?;
                    } else {
                        let new_value = ppu_address_space.read_le_value::<u8>(
                            self.state.vram_address_pointer as usize,
                            self.timestamp,
                            None,
                        )?;

                        *buffer = self
                            .state
                            .vram_read_buffer
                            .swap(new_value, Ordering::AcqRel);
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
                    self.state.show_background_leftmost_pixels = *buffer & 0b0000_0010 != 0;
                    self.state.oam.show_sprites_leftmost_pixels = *buffer & 0b0000_0100 != 0;
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

                    self.state.vram_address_pointer_write_phase =
                        !self.state.vram_address_pointer_write_phase;

                    // Write the completed address
                    if !self.state.vram_address_pointer_write_phase {
                        self.state.vram_address_pointer = self.state.shadow_vram_address_pointer;
                    }
                }
                CpuAccessibleRegister::PpuData => {
                    tracing::trace!(
                        "CPU is sending data to 0x{:04x} in the PPU address space: {:02x}, the \
                         cycle counter is at {}",
                        self.state.vram_address_pointer,
                        buffer,
                        self.state.cycle_counter
                    );

                    let runtime = RuntimeApi::current();
                    let ppu_address_space = runtime.address_space(self.ppu_address_space).unwrap();

                    // Redirect into the ppu address space
                    ppu_address_space.write_le_value(
                        self.state.vram_address_pointer as usize,
                        self.timestamp,
                        self.ppu_address_space_cache.as_mut(),
                        *buffer,
                    )?;

                    self.state.vram_address_pointer =
                        self.state.vram_address_pointer.wrapping_add(u16::from(
                            self.state.vram_address_pointer_increment_amount,
                        )) & 0b0111_1111_1111_1111;
                }
                CpuAccessibleRegister::OamDma => {
                    let runtime = RuntimeApi::current();
                    let cpu_address_space = runtime.address_space(self.cpu_address_space).unwrap();

                    let page = u16::from(*buffer) << 8;

                    self.processor_rdy.as_ref().unwrap().store(false);

                    // TODO: Extract to constant or extract from cpu directly within the config builder
                    let processor_frequency = R::master_clock() / 12;

                    let next_processor_rdy_high =
                        self.timestamp + (processor_frequency.recip() * 514);

                    // Make sure we wake up eventually
                    runtime.insert_event(
                        self.framebuffer_path.parent().unwrap(),
                        WAKEUP_CPU_VIA_RDY,
                        next_processor_rdy_high,
                        EventRequeueMode::Once,
                        EventType::sync_point(),
                    );

                    // Read off OAM data immediately, this is done for performance and should not
                    // have any side effects
                    let _ = cpu_address_space.read(
                        page as usize,
                        self.timestamp,
                        None,
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

    fn handle_event(&mut self, name: &str, event: EventType) {
        match event {
            EventType::SyncPoint => match name {
                VBLANK_START => {
                    self.state.entered_vblank.store(true, Ordering::Release);

                    if self.state.vblank_nmi_enabled {
                        self.processor_nmi.as_ref().unwrap().store(false);
                    }
                }
                VBLANK_END => {
                    let runtime = RuntimeApi::current();

                    self.state.entered_vblank.store(false, Ordering::Release);
                    self.processor_nmi.as_ref().unwrap().store(true);

                    runtime.commit_framebuffer::<G>(&self.framebuffer_path, |framebuffer| {
                        self.backend
                            .as_mut()
                            .unwrap()
                            .commit_staging_buffer(&self.staging_buffer, framebuffer);
                    });
                }
                WAKEUP_CPU_VIA_RDY => {
                    self.processor_rdy.as_ref().unwrap().store(true);
                }
                _ => {
                    unreachable!("{}", name)
                }
            },
            _ => {
                unreachable!()
            }
        }
    }

    fn synchronize(&mut self, mut context: SynchronizationContext) {
        let runtime = RuntimeApi::current();
        let ppu_address_space = runtime.address_space(self.ppu_address_space).unwrap();

        for now in context.allocate(self.period, None) {
            self.timestamp = now;

            if self.state.cycle_counter.y == 261 {
                if self.state.cycle_counter.x == 257 && self.state.background.rendering_enabled {
                    let t =
                        VramAddressPointerContents::from(self.state.shadow_vram_address_pointer);
                    let mut v = VramAddressPointerContents::from(self.state.vram_address_pointer);

                    v.nametable.x = t.nametable.x;
                    v.coarse.x = t.coarse.x;

                    self.state.vram_address_pointer = u16::from(v);
                }

                if let 280..=304 = self.state.cycle_counter.x
                    && self.state.background.rendering_enabled
                {
                    let t =
                        VramAddressPointerContents::from(self.state.shadow_vram_address_pointer);
                    let mut v = VramAddressPointerContents::from(self.state.vram_address_pointer);

                    v.nametable.y = t.nametable.y;
                    v.coarse.y = t.coarse.y;
                    v.fine_y = t.fine_y;

                    self.state.vram_address_pointer = u16::from(v);
                }

                if let 305..=320 = self.state.cycle_counter.x
                    && self.state.background.rendering_enabled
                {
                    self.state.drive_background_pipeline::<R>(
                        ppu_address_space,
                        self.ppu_address_space_cache.as_mut().unwrap(),
                        self.timestamp,
                    );
                }
            }

            if (0..R::VISIBLE_SCANLINES).contains(&self.state.cycle_counter.y) {
                if self.state.cycle_counter.x == 1 {
                    // Technically the NES does it over 64 cycles
                    self.state.oam.secondary_data.clear();
                }

                if let 1..=256 = self.state.cycle_counter.x {
                    let scanline_position_x = self.state.cycle_counter.x - 1;

                    self.state.drive_background_pipeline::<R>(
                        ppu_address_space,
                        self.ppu_address_space_cache.as_mut().unwrap(),
                        self.timestamp,
                    );

                    let mut sprite_color_index = None;

                    let potential_sprite = self
                        .state
                        .oam
                        .currently_rendering_sprites
                        .iter()
                        .rev()
                        .find_map(|sprite| {
                            let in_sprite_position = scanline_position_x
                                .checked_sub(u16::from(sprite.oam.position.x))?;

                            if in_sprite_position < 8 {
                                let in_sprite_position = if !sprite.oam.flip.x {
                                    in_sprite_position
                                } else {
                                    7 - in_sprite_position
                                };

                                let low =
                                    (sprite.pattern_table_low >> (7 - in_sprite_position)) & 1;
                                let high =
                                    (sprite.pattern_table_high >> (7 - in_sprite_position)) & 1;

                                let color_index = (high << 1) | low;

                                if color_index != 0 {
                                    Some((sprite, color_index))
                                } else {
                                    None
                                }
                            } else {
                                None
                            }
                        });

                    if let Some((sprite, color_index)) = potential_sprite {
                        sprite_color_index = Some(self.state.calculate_sprite_color::<R>(
                            ppu_address_space,
                            self.ppu_address_space_cache.as_mut().unwrap(),
                            self.timestamp,
                            sprite.oam,
                            color_index,
                        ));
                    }

                    let bit_position =
                        15 - self.state.background.fine_x_scroll - self.state.background.tile_pixel;

                    let high = (self.state.background.pattern_high_shift >> bit_position) & 1;
                    let low = (self.state.background.pattern_low_shift >> bit_position) & 1;

                    let attribute = (self.state.background.attribute_shift
                        >> (30 - self.state.background.tile_pixel * 2))
                        & 0b11;

                    self.state.background.tile_pixel += 1;
                    if self.state.background.tile_pixel == 8 {
                        self.state.background.tile_pixel = 0;
                    }

                    let color_index = (high << 1) | low;

                    let background_color_index = self.state.calculate_background_color::<R>(
                        ppu_address_space,
                        self.ppu_address_space_cache.as_mut().unwrap(),
                        self.timestamp,
                        attribute as u8,
                        color_index as u8,
                    );

                    let color_index = if self.state.oam.rendering_enabled {
                        sprite_color_index
                    } else {
                        None
                    }
                    .or(if self.state.background.rendering_enabled {
                        Some(background_color_index)
                    } else {
                        None
                    })
                    .unwrap_or(PPU_BLACK_INDEX);

                    let point = Point2::new(scanline_position_x, self.state.cycle_counter.y);

                    self.staging_buffer[point.cast()] = color_index;
                }

                if let 65..=256 = self.state.cycle_counter.x {
                    let sprite_index = (self.state.cycle_counter.x - 65) / 2;
                    let oam_data_index = sprite_index * 4;

                    if sprite_index < 64 {
                        match self.state.oam.sprite_evaluation_state {
                            SpriteEvaluationState::InspectingY => {
                                let sprite_y = self.state.oam.data[oam_data_index as usize];

                                self.state.oam.sprite_evaluation_state =
                                    SpriteEvaluationState::Evaluating { sprite_y };
                            }
                            SpriteEvaluationState::Evaluating { sprite_y } => {
                                if (u16::from(sprite_y)..u16::from(sprite_y) + 8)
                                    .contains(&(self.state.cycle_counter.y))
                                {
                                    let mut bytes = [0; 4];
                                    bytes.copy_from_slice(
                                        &self.state.oam.data[RangeInclusive::from_start_and_length(
                                            oam_data_index as usize,
                                            4,
                                        )],
                                    );

                                    let sprite = OamSprite::from_bytes(bytes);

                                    if self.state.oam.secondary_data.push(sprite).is_err() {
                                        // TODO: Handle sprite overflow flag
                                    }
                                }

                                self.state.oam.sprite_evaluation_state =
                                    SpriteEvaluationState::InspectingY;
                            }
                        }
                    }
                }

                if self.state.cycle_counter.x == 256 && self.state.background.rendering_enabled {
                    let mut vram_address_pointer_contents =
                        VramAddressPointerContents::from(self.state.vram_address_pointer);

                    if vram_address_pointer_contents.fine_y == 7 {
                        vram_address_pointer_contents.fine_y = 0;

                        if vram_address_pointer_contents.coarse.y == 29 {
                            vram_address_pointer_contents.coarse.y = 0;

                            vram_address_pointer_contents.nametable.y =
                                !vram_address_pointer_contents.nametable.y;
                        } else if vram_address_pointer_contents.coarse.y == 31 {
                            vram_address_pointer_contents.coarse.y = 0;
                        } else {
                            vram_address_pointer_contents.coarse.y += 1;
                        }
                    } else {
                        vram_address_pointer_contents.fine_y += 1;
                    }

                    self.state.vram_address_pointer = u16::from(vram_address_pointer_contents);
                }

                if self.state.cycle_counter.x == 257 {
                    self.state.oam.currently_rendering_sprites.clear();

                    if self.state.background.rendering_enabled {
                        let t = VramAddressPointerContents::from(
                            self.state.shadow_vram_address_pointer,
                        );
                        let mut v =
                            VramAddressPointerContents::from(self.state.vram_address_pointer);

                        v.nametable.x = t.nametable.x;
                        v.coarse.x = t.coarse.x;

                        self.state.vram_address_pointer = u16::from(v);
                    }
                }

                if let 257..=320 = self.state.cycle_counter.x {
                    self.state.drive_sprite_pipeline::<R>(
                        ppu_address_space,
                        self.ppu_address_space_cache.as_mut().unwrap(),
                        self.timestamp,
                    );
                }

                if let 321..=336 = self.state.cycle_counter.x {
                    self.state.drive_background_pipeline::<R>(
                        ppu_address_space,
                        self.ppu_address_space_cache.as_mut().unwrap(),
                        self.timestamp,
                    );
                }
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
