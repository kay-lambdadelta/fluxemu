use alloc::boxed::Box;
use core::{marker::PhantomData, ops::RangeInclusive};

use fluxemu_definition_mos6502::{Mos6502, Mos6502Event, Pin, variant::Ricoh2A0x};
use fluxemu_graphics::texture::{AsViewTexture, OwnedTexture, Texture};
use fluxemu_math::range::ContiguousRange;
use fluxemu_runtime::{
    ResourcePath, RuntimeHandle,
    component::{
        Component,
        config::{ComponentConfig, LateContext},
    },
    graphics::SimpleDisplayBackend,
    machine::builder::ComponentBuilder,
    memory::{Address, AddressSpaceId, MemoryError, MemoryMapCommand, Permissions},
    path::ComponentPath,
    platform::Platform,
    scheduler::{
        Period,
        event::{Event, EventMode, downcast_event},
        task::{FrequencyBased, Mode},
    },
};
use nalgebra::Point2;
use palette::{Srgb, Srgba, named::BLACK};
use serde::{Deserialize, Serialize};
use strum::FromRepr;

use crate::ppu::{
    backend::SupportedGraphicsApi,
    background::BackgroundState,
    oam::{OamState, SpriteEvaluationState},
    region::Region,
    state::{State, VramAddressPointerContents},
};

pub mod backend;
mod background;
mod color;
mod oam;
mod pipeline;
pub mod region;
mod state;

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
pub const PALETTE_RAM_ADDRESSES: RangeInclusive<Address> = BACKGROUND_PALETTE_BASE_ADDRESS..=0x3f1f;
pub const ATTRIBUTE_BASE_ADDRESS: Address = NAMETABLE_BASE_ADDRESS + 0x3c0;
const DUMMY_SCANLINE_COUNT: u16 = 2;
const VISIBLE_SCANLINE_LENGTH: u16 = 256;
const HBLANK_LENGTH: u16 = 85;
const TOTAL_SCANLINE_LENGTH: u16 = VISIBLE_SCANLINE_LENGTH + HBLANK_LENGTH;
const NMI_ARM_DELAY: u128 = 2;
const VRAM_ADDRESS_WRITE_DELAY: u128 = 3;
const SPRITE_FLAG_READ_LATENCY: u16 = 1;
const NMI_ENABLE_CUTOFF: u128 = 1;
const IO_LATCH_DECAY_FRAMES: u32 = 36;

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
pub struct Ppu<R: Region, G: SupportedGraphicsApi> {
    state: State,
    backend: Option<G::Backend<R>>,
    cpu_address_space: AddressSpaceId,
    ppu_address_space: AddressSpaceId,
    processor_path: ComponentPath,
    staging_buffer: OwnedTexture<Srgba<u8>>,
    palette: [[Srgb<u8>; 64]; 8],
    path: ComponentPath,
    framebuffer_path: ResourcePath,
    period: Period,
}

impl<R: Region, P: Platform<GraphicsApi: SupportedGraphicsApi>> ComponentConfig<P>
    for PpuConfig<R>
{
    type Component = Ppu<R, P::GraphicsApi>;

    fn late_initialize(component: &mut Self::Component, data: &LateContext<P>) {
        let backend = <P::GraphicsApi as SupportedGraphicsApi>::Backend::new(
            data.graphics_initialization_data.clone(),
        );
        component.backend = Some(backend);
    }

    fn build_component(
        self,
        component_builder: ComponentBuilder<P, Self::Component>,
    ) -> Result<Self::Component, Box<dyn core::error::Error>> {
        let frequency = R::master_clock() / R::PPU_CLOCK_DIVISOR as u128;

        let (component_builder, framebuffer_path) = component_builder
            .bus_master(self.cpu_address_space)
            .bus_master(self.ppu_address_space)
            .task(
                "synchronization",
                Mode::OnDemand,
                FrequencyBased::new(frequency, Self::Component::task),
            )
            .framebuffer("framebuffer", |component, path| {
                component
                    .backend
                    .as_mut()
                    .unwrap()
                    .produce_initial_framebuffer(path)
            });

        let my_path = component_builder.path().clone();

        let register_mappings = MemoryMapCommand::with_component(
            my_path.clone(),
            [
                (
                    CpuAccessibleRegister::PpuCtrl as usize
                        ..=CpuAccessibleRegister::PpuData as usize,
                    Permissions::ALL,
                ),
                (
                    RangeInclusive::from_single(CpuAccessibleRegister::OamDma as usize),
                    Permissions::WRITE,
                ),
            ],
        );

        let time = TOTAL_SCANLINE_LENGTH as u32 * (R::VISIBLE_SCANLINES as u32 + 2);
        let component_builder = component_builder
            .map_memory(self.cpu_address_space, register_mappings)
            .schedule_event::<Self::Component>(
                // x: 1, y: 241
                &my_path,
                Period::from_num(time) / frequency,
                EventMode::Once,
                PpuEvent::VblankStart,
            );

        let staging_buffer = Texture::from_value(
            VISIBLE_SCANLINE_LENGTH as usize,
            R::VISIBLE_SCANLINES as usize,
            BLACK.into(),
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
                cycle_counter: Point2::new(1, R::PRERENDER_SCANLINE),
                oam: OamState {
                    data: core::array::from_fn(|index| {
                        let value: u8 = rand::random();

                        if index % 4 == 2 {
                            value & 0b1110_0011
                        } else {
                            value
                        }
                    }),
                    oam_addr: 0x00,
                    sprite_evaluation_state: SpriteEvaluationState::Searching,
                    secondary_data: [0xff; 32],
                    secondary_address: 0,
                    bus_latch: 0xff,
                    sprite_zero_in_secondary: false,
                    currently_rendering_sprites: heapless::Vec::new(),
                    show_leftmost_pixels: true,
                    sprite_8x8_pattern_table_index: 0x0000,
                    rendering_enabled: false,
                    fetched_pattern_table_low: 0,
                    sprite_zero_hit: false,
                    sprite_8x16_mode: false,
                    overflow: false,
                },
                background: BackgroundState {
                    pattern_table_index: 0x0000,
                    pattern_low_shift: 0,
                    pattern_high_shift: 0,
                    attribute_shift: 0,
                    fine_x_scroll: 0,
                    rendering_enabled: false,
                    show_leftmost_pixels: true,
                    nametable_address_low: 0,
                    fetched_nametable: 0,
                    fetched_attribute: 0,
                    fetched_pattern_table_low: 0,
                },
                vram_address_pointer: 0,
                shadow_vram_address_pointer: 0,
                odd_frame: false,
                skipped_dot_this_frame: false,
                rendering_enabled_for_skip: false,
                emphasis_index: 0,
                frame_count: 0,
                io_latch: 0,
                io_latch_refresh_frames: [0; 8],
                nmi_armed: false,
                nmi_line_low: false,
                suppress_vblank: false,
                next_vblank_timestamp: Period::from_num(time) / frequency,
                vblank_end_timestamp: Period::ZERO,
            },
            backend: None,
            staging_buffer,
            cpu_address_space: self.cpu_address_space,
            processor_path: self.processor.clone(),
            ppu_address_space: self.ppu_address_space,
            palette: R::generate_palette(),
            path: component_builder.path().clone(),
            period: frequency.recip(),
            framebuffer_path,
        })
    }
}

impl<R: Region, G: SupportedGraphicsApi> Component for Ppu<R, G> {
    type Event = PpuEvent;

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
                CpuAccessibleRegister::PpuStatus => {
                    let sprite_flags_clearing = self.state.cycle_counter.y == R::PRERENDER_SCANLINE
                        && self.state.cycle_counter.x <= SPRITE_FLAG_READ_LATENCY;

                    let value = (self.state.io_latch & 0b0001_1111)
                        | ((self.state.entered_vblank as u8) << 7)
                        | (((self.state.oam.sprite_zero_hit && !sprite_flags_clearing) as u8) << 6)
                        | (((self.state.oam.overflow && !sprite_flags_clearing) as u8) << 5);

                    *buffer = value;

                    if !avoid_side_effects {
                        self.state.vram_address_pointer_write_phase = false;
                        self.state.entered_vblank = false;
                        self.refresh_io_latch(value, 0b1110_0000);

                        RuntimeHandle::with_current(|runtime| {
                            let timestamp = runtime.current_timestamp();
                            let vblank_timestamp = self.state.next_vblank_timestamp;

                            if timestamp < vblank_timestamp
                                && vblank_timestamp - timestamp <= self.period
                            {
                                self.state.suppress_vblank = true;
                            }

                            self.update_nmi_line(runtime);
                        });
                    }
                }
                CpuAccessibleRegister::OamData => {
                    let value = if self.is_evaluating_sprites() {
                        self.oam_bus_value()
                    } else {
                        self.state.oam.data[self.state.oam.oam_addr as usize]
                    };

                    *buffer = value;

                    if !avoid_side_effects {
                        self.refresh_io_latch(value, 0b1111_1111);
                    }
                }
                CpuAccessibleRegister::PpuData => {
                    RuntimeHandle::with_current(|runtime| {
                        let timestamp = runtime.current_timestamp();

                        let mut ppu_address_space =
                            runtime.address_space(self.ppu_address_space).unwrap();

                        let vram_address_pointer = self.state.vram_address_pointer as usize;

                        if avoid_side_effects {
                            *buffer = ppu_address_space
                                .read_le_value::<_, true>(vram_address_pointer, timestamp)
                                .unwrap_or_default();
                        } else {
                            // RGB models do not have this quirk
                            let bypass_read_buffer = R::BYPASS_READ_BUFFER_FOR_PPUDATA_PALETTE_READS
                                // Cover the ENTIRE range, including the mirrors
                                && vram_address_pointer & 0x3fff
                                    >= *PALETTE_RAM_ADDRESSES.start();

                            if bypass_read_buffer {
                                let data: u8 = ppu_address_space
                                    .read_le_value::<_, false>(vram_address_pointer, timestamp)
                                    .unwrap_or_default();
                                let color_mask = if self.state.greyscale {
                                    0b0011_0000
                                } else {
                                    0b0011_1111
                                };

                                *buffer = (self.state.io_latch & 0b1100_0000) | (data & color_mask);
                                self.refresh_io_latch(*buffer, 0b0011_1111);

                                // HACK: The memory subsystem does not (yet) support dealing with memory shadowing
                                //
                                // This should work, but it is a hack
                                let underlying_address = (vram_address_pointer & 0x0fff) | 0x2000;
                                self.state.vram_read_buffer = ppu_address_space
                                    .read_le_value::<_, false>(underlying_address, timestamp)
                                    .unwrap_or_default();
                            } else {
                                let data = ppu_address_space
                                    .read_le_value::<_, false>(vram_address_pointer, timestamp)
                                    .unwrap_or_default();

                                // Delay the read through the buffer
                                *buffer =
                                    core::mem::replace(&mut self.state.vram_read_buffer, data);
                                self.refresh_io_latch(*buffer, 0b1111_1111);
                            }

                            self.increment_vram_address_pointer();
                        }
                    });
                }
                CpuAccessibleRegister::PpuCtrl
                | CpuAccessibleRegister::PpuMask
                | CpuAccessibleRegister::OamAddr
                | CpuAccessibleRegister::PpuScroll
                | CpuAccessibleRegister::PpuAddr => {
                    *buffer = self.state.io_latch;
                }
                CpuAccessibleRegister::OamDma => {
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

            if !matches!(register, CpuAccessibleRegister::OamDma) {
                self.refresh_io_latch(*buffer, 0b1111_1111);
            }

            match register {
                CpuAccessibleRegister::PpuStatus => {}
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
                    self.state.oam.sprite_8x16_mode = *buffer & 0b0010_0000 != 0;

                    self.state.vblank_nmi_enabled = *buffer & 0b1000_0000 != 0;
                    self.state.shadow_vram_address_pointer = shadow_vram_address_pointer.into();

                    RuntimeHandle::with_current(|runtime| {
                        let timestamp = runtime.current_timestamp();
                        let vblank_end_timestamp = self.state.vblank_end_timestamp;

                        if !self.state.nmi_line_low
                            && timestamp < vblank_end_timestamp
                            && vblank_end_timestamp - timestamp <= self.period * NMI_ENABLE_CUTOFF
                        {
                            self.state.nmi_armed = false;
                        }

                        self.update_nmi_line(runtime);
                    });
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
                    self.state.emphasis_index = *buffer >> 5;
                }
                CpuAccessibleRegister::OamAddr => {
                    self.state.oam.oam_addr = *buffer;
                }
                CpuAccessibleRegister::OamData => {
                    if self.rendering_enabled() && self.is_on_rendering_scanline() {
                        self.state.oam.oam_addr =
                            self.state.oam.oam_addr.wrapping_add(4) & 0b1111_1100;
                    } else {
                        self.state.oam.write(self.state.oam.oam_addr, *buffer);
                        self.state.oam.oam_addr = self.state.oam.oam_addr.wrapping_add(1);
                    }
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

                    if self.state.vram_address_pointer_write_phase {
                        unpacked_address[1] = *buffer;
                    } else {
                        unpacked_address[0] = *buffer & 0b0011_1111;
                    }

                    self.state.shadow_vram_address_pointer =
                        u16::from_be_bytes(unpacked_address) & 0b0111_1111_1111_1111;

                    // Write the completed address
                    if self.state.vram_address_pointer_write_phase {
                        RuntimeHandle::with_current(|runtime| {
                            runtime.schedule_event_relative::<Self>(
                                &self.path,
                                EventMode::Once,
                                self.period * VRAM_ADDRESS_WRITE_DELAY,
                                PpuEvent::ApplyVramAddress,
                            );
                        });
                    }

                    self.state.vram_address_pointer_write_phase =
                        !self.state.vram_address_pointer_write_phase;
                }
                CpuAccessibleRegister::PpuData => {
                    RuntimeHandle::with_current(|runtime| {
                        let timestamp = runtime.current_timestamp();

                        let mut ppu_address_space =
                            runtime.address_space(self.ppu_address_space).unwrap();

                        // Redirect into the ppu address space
                        let _ = ppu_address_space.write_le_value(
                            self.state.vram_address_pointer as usize,
                            timestamp,
                            *buffer,
                        );

                        self.increment_vram_address_pointer();
                    });
                }
                CpuAccessibleRegister::OamDma => {
                    RuntimeHandle::with_current(|runtime| {
                        let timestamp = runtime.current_timestamp();

                        let mut cpu_address_space =
                            runtime.address_space(self.cpu_address_space).unwrap();

                        let page = u16::from(*buffer) << 8;

                        runtime.schedule_event_now::<Mos6502<Ricoh2A0x>>(
                            &self.processor_path,
                            EventMode::Once,
                            Mos6502Event::FlagChange {
                                pin: Pin::Rdy,
                                value: false,
                            },
                        );

                        let processor_frequency = R::master_clock() / R::CPU_CLOCK_DIVISOR as u128;

                        // Make sure the cpu wakes up
                        runtime.schedule_event_relative::<Mos6502<Ricoh2A0x>>(
                            &self.processor_path,
                            EventMode::Once,
                            processor_frequency.recip() * 514,
                            Mos6502Event::FlagChange {
                                pin: Pin::Rdy,
                                value: true,
                            },
                        );

                        // Read off OAM data immediately, this is done for performance and should not
                        // have any side effects
                        let mut data = [0; 256];

                        if (0x2000..0x4000).contains(&page) {
                            drop(cpu_address_space);

                            for (offset, value) in data.iter_mut().enumerate() {
                                let register = 0x2000 | (offset & 0b111);

                                let _ = self.memory_read(
                                    register,
                                    self.cpu_address_space,
                                    false,
                                    core::slice::from_mut(value),
                                );
                            }
                        } else {
                            let _ = cpu_address_space.read::<_, false>(
                                page as usize,
                                timestamp,
                                &mut data,
                            );
                        }

                        for (offset, value) in data.into_iter().enumerate() {
                            let address = self.state.oam.oam_addr.wrapping_add(offset as u8);

                            self.state.oam.write(address, value);
                        }
                    });
                }
            }
        }

        Ok(())
    }

    fn handle_event(&mut self, event: Box<dyn Event>) {
        let event = downcast_event::<Self>(event);

        RuntimeHandle::with_current(|runtime| match event {
            PpuEvent::VblankStart => {
                if !core::mem::take(&mut self.state.suppress_vblank) {
                    self.state.entered_vblank = true;

                    runtime.schedule_event_relative::<Self>(
                        &self.path,
                        EventMode::Once,
                        self.period * NMI_ARM_DELAY,
                        PpuEvent::ArmNmi,
                    );
                }

                let vblank_len =
                    Period::from_num(TOTAL_SCANLINE_LENGTH as u128 * R::VBLANK_LENGTH as u128);

                let delay = self.period * vblank_len;
                self.state.vblank_end_timestamp = runtime.current_timestamp() + delay;

                runtime.schedule_event_relative::<Self>(
                    &self.path,
                    EventMode::Once,
                    delay,
                    PpuEvent::VblankEnd,
                );
            }
            PpuEvent::ApplyVramAddress => {
                self.state.vram_address_pointer = self.state.shadow_vram_address_pointer;
            }
            PpuEvent::ArmNmi => {
                self.state.nmi_armed = true;
                self.update_nmi_line(runtime);
            }
            PpuEvent::VblankEnd => {
                self.state.entered_vblank = false;
                self.state.nmi_armed = false;
                self.state.odd_frame = !self.state.odd_frame;
                self.update_nmi_line(runtime);
                self.decay_io_latch();

                runtime.write_framebuffer::<G, _>(&self.framebuffer_path, |framebuffer| {
                    self.backend
                        .as_mut()
                        .unwrap()
                        .commit_staging_buffer(self.staging_buffer.as_view(), framebuffer);
                });

                runtime.schedule_event_relative::<Self>(
                    &self.path,
                    EventMode::Once,
                    self.period * TOTAL_SCANLINE_LENGTH as u128,
                    PpuEvent::FrameLengthDecided,
                );
            }
            PpuEvent::FrameLengthDecided => {
                let lines_until_next_vblank = R::TOTAL_SCANLINES - R::VBLANK_LENGTH - 1;
                let mut cycles_until_next_vblank =
                    TOTAL_SCANLINE_LENGTH as u128 * lines_until_next_vblank as u128;

                if core::mem::take(&mut self.state.skipped_dot_this_frame) {
                    cycles_until_next_vblank -= 1;
                }

                let delay = self.period * cycles_until_next_vblank;
                self.state.next_vblank_timestamp = runtime.current_timestamp() + delay;

                runtime.schedule_event_relative::<Self>(
                    &self.path,
                    EventMode::Once,
                    delay,
                    PpuEvent::VblankStart,
                );
            }
        });
    }
}

impl<R: Region, G: SupportedGraphicsApi> Ppu<R, G> {
    fn update_nmi_line(&mut self, runtime: &RuntimeHandle) {
        let line_low =
            self.state.nmi_armed && self.state.entered_vblank && self.state.vblank_nmi_enabled;

        if line_low != self.state.nmi_line_low {
            self.state.nmi_line_low = line_low;

            runtime.schedule_event_now::<Mos6502<Ricoh2A0x>>(
                &self.processor_path,
                EventMode::Once,
                Mos6502Event::FlagChange {
                    pin: Pin::Nmi,
                    value: !line_low,
                },
            );
        }
    }

    fn refresh_io_latch(&mut self, value: u8, mask: u8) {
        self.state.io_latch = (self.state.io_latch & !mask) | (value & mask);

        for (bit, refresh_frame) in self.state.io_latch_refresh_frames.iter_mut().enumerate() {
            if mask & (1 << bit) != 0 {
                *refresh_frame = self.state.frame_count;
            }
        }
    }

    fn decay_io_latch(&mut self) {
        self.state.frame_count = self.state.frame_count.wrapping_add(1);

        for (bit, refresh_frame) in self.state.io_latch_refresh_frames.iter().enumerate() {
            if self.state.frame_count.wrapping_sub(*refresh_frame) > IO_LATCH_DECAY_FRAMES {
                self.state.io_latch &= !(1 << bit);
            }
        }
    }
}

#[derive(Debug, Clone)]
pub enum PpuEvent {
    VblankStart,
    ArmNmi,
    VblankEnd,
    FrameLengthDecided,
    ApplyVramAddress,
}
