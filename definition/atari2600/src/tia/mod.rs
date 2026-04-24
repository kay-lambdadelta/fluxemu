use std::{
    any::Any,
    collections::{HashMap, HashSet},
    fmt::Debug,
};

pub(crate) use backend::SupportedGraphicsApiTia;
use color::TiaColor;
use fluxemu_runtime::{
    ComponentPath,
    component::Component,
    graphics::software::Texture,
    memory::{Address, AddressSpaceId, MemoryError},
    scheduler::{Period, SynchronizationContext},
};
use nalgebra::Point2;
use palette::Srgba;
use region::Region;
use serde::{Deserialize, Serialize};

use crate::tia::{
    backend::TiaDisplayBackend,
    memory::{ReadRegisters, WriteRegisters},
};

mod backend;
mod color;
pub(crate) mod config;
mod memory;
pub mod region;

const HBLANK_LENGTH: u16 = 68;
const VISIBLE_SCANLINE_LENGTH: u16 = 160;
const SCANLINE_LENGTH: u16 = HBLANK_LENGTH + VISIBLE_SCANLINE_LENGTH;

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Clone, Copy, Serialize, Deserialize)]
enum ObjectId {
    Player0,
    Player1,
    Missile0,
    Missile1,
    Playfield,
    Ball,
}

#[derive(Default, Debug, PartialEq, Eq, Clone, Copy, Serialize, Deserialize)]
enum InputControl {
    #[default]
    Normal,
    LatchedOrDumped,
}

#[derive(Default, Debug, Serialize, Deserialize)]
struct Missile {
    position: u16,
    enabled: bool,
    motion: i8,
    color: TiaColor,
    /// Locked to player and invisible
    locked: bool,
}

#[derive(Default, Debug, Serialize, Deserialize)]
enum DelayEnableChangeBall {
    #[default]
    Disabled,
    Enabled(Option<bool>),
}

#[derive(Default, Debug, Serialize, Deserialize)]
struct Ball {
    position: u16,
    enabled: bool,
    delay_enable_change: DelayEnableChangeBall,
    motion: i8,
    color: TiaColor,
    size: u8,
}

#[derive(Default, Debug, Serialize, Deserialize)]
struct Playfield {
    mirror: bool,
    color: TiaColor,
    score_mode: bool,
    // 20 bits
    data: [bool; 20],
}

#[derive(Default, Debug, Serialize, Deserialize)]
enum DelayChangeGraphicPlayer {
    #[default]
    Disabled,
    Enabled(Option<u8>),
}

#[derive(Default, Debug, Serialize, Deserialize)]
struct Player {
    position: u16,
    graphic: u8,
    mirror: bool,
    delay_change_graphic: DelayChangeGraphicPlayer,
    motion: i8,
    color: TiaColor,
}

#[derive(Debug, Serialize, Deserialize)]
struct State {
    collision_matrix: HashMap<ObjectId, HashSet<ObjectId>>,
    vblank_active: bool,
    cycles_waiting_for_vsync: Option<u16>,
    input_control: [InputControl; 6],
    electron_beam: Point2<u16>,
    missiles: [Missile; 2],
    ball: Ball,
    players: [Player; 2],
    playfield: Playfield,
    high_playfield_ball_priority: bool,
    background_color: TiaColor,
    staging_buffer: Texture<Srgba<u8>>,
}

#[derive(Debug)]
pub(crate) struct Tia<R: Region, G: SupportedGraphicsApiTia> {
    state: State,
    backend: Option<G::Backend<R>>,
    cpu_path: ComponentPath,
    path: ComponentPath,
}

impl<R: Region, G: SupportedGraphicsApiTia> Component for Tia<R, G> {
    type Event = ();

    fn memory_read(
        &mut self,
        address: Address,
        _address_space: AddressSpaceId,
        _avoid_side_effects: bool,
        buffer: &mut [u8],
    ) -> Result<(), MemoryError> {
        let data = &mut buffer[0];

        if let Some(address) = ReadRegisters::from_repr(address as u16) {
            tracing::trace!("Reading from TIA register: {:?}", address);

            self.handle_read_register(data, address);

            Ok(())
        } else {
            unreachable!("{:x}", address);
        }
    }

    fn memory_write(
        &mut self,
        address: Address,
        _address_space: AddressSpaceId,
        buffer: &[u8],
    ) -> Result<(), MemoryError> {
        let data = buffer[0];

        if let Some(address) = WriteRegisters::from_repr(address as u16) {
            tracing::trace!("Writing to TIA register: {:?} = {:02x}", address, data);

            self.handle_write_register(data, address);

            Ok(())
        } else {
            unreachable!("{:x}", address);
        }
    }

    fn synchronize(&mut self, mut context: SynchronizationContext) {
        for _ in context.allocate(R::frequency().recip()) {
            if let Some(cycles) = self.state.cycles_waiting_for_vsync {
                self.state.cycles_waiting_for_vsync = Some(cycles.saturating_sub(1));

                if self.state.cycles_waiting_for_vsync == Some(0) {
                    self.backend
                        .as_mut()
                        .unwrap()
                        .commit_staging_buffer(&self.state.staging_buffer);

                    self.state.cycles_waiting_for_vsync = None;
                }
            }

            if !(self.state.cycles_waiting_for_vsync.is_some() || self.state.vblank_active)
                && (HBLANK_LENGTH..(VISIBLE_SCANLINE_LENGTH + HBLANK_LENGTH))
                    .contains(&self.state.electron_beam.x)
            {
                let color = R::color_to_srgb(self.get_rendered_color());

                let point = Point2::new(
                    (self.state.electron_beam.x - HBLANK_LENGTH) as usize,
                    self.state.electron_beam.y as usize,
                );

                self.state.staging_buffer[point] = color.into();
            }

            self.state.electron_beam.x += 1;

            if self.state.electron_beam.x >= SCANLINE_LENGTH {
                self.state.electron_beam.x = 0;
                self.state.electron_beam.y += 1;
            }

            if self.state.electron_beam.y >= R::TOTAL_SCANLINES {
                self.state.electron_beam.y = 0;
            }
        }
    }

    fn needs_work(&self, _timestamp: &Period, delta: &Period) -> bool {
        *delta >= R::frequency().recip()
    }

    fn get_framebuffer(&mut self, _name: &str) -> &dyn Any {
        self.backend.as_ref().unwrap().framebuffer()
    }
}

impl<R: Region, G: SupportedGraphicsApiTia> Tia<R, G> {
    fn get_rendered_color(&self) -> TiaColor {
        if self.state.high_playfield_ball_priority {
            // Check if in the bounds of ball
            if self.get_ball_color() {
                return self.state.ball.color;
            }

            // Check if in the bounds of playfield
            if let Some(color) = self.get_playfield_color() {
                return color;
            }

            // Check if in the bounds of player 0
            if let Some(color) = self.get_player_color(0) {
                return color;
            }

            // Check if in the bounds of player 1
            if let Some(color) = self.get_player_color(1) {
                return color;
            }

            // Check if in the bounds of missile 0
            if self.get_missile_color(0) {
                return self.state.missiles[0].color;
            }

            // Check if in the bounds of missile 1
            if self.get_missile_color(1) {
                return self.state.missiles[1].color;
            }
        } else {
            // Check if in the bounds of player 0
            if let Some(color) = self.get_player_color(0) {
                return color;
            }

            // Check if in the bounds of player 1
            if let Some(color) = self.get_player_color(1) {
                return color;
            }

            // Check if in the bounds of missile 0
            if self.get_missile_color(0) {
                return self.state.missiles[0].color;
            }

            // Check if in the bounds of missile 1
            if self.get_missile_color(1) {
                return self.state.missiles[1].color;
            }

            // Check if in the bounds of ball
            if self.get_ball_color() {
                return self.state.ball.color;
            }

            // Check if in the bounds of playfield
            if let Some(color) = self.get_playfield_color() {
                return color;
            }
        }

        self.state.background_color
    }

    #[inline]
    fn get_player_color(&self, index: usize) -> Option<TiaColor> {
        let player = &self.state.players[index];
        if let Some(sprite_pixel) = self
            .state
            .electron_beam
            .x
            .checked_sub(player.position)
            .map(usize::from)
            && sprite_pixel < 8
        {
            let bit = if player.mirror {
                player.graphic & (1 << sprite_pixel) != 0
            } else {
                player.graphic & (1 << (7 - sprite_pixel)) != 0
            };

            return if bit { Some(player.color) } else { None };
        }

        None
    }

    #[inline]
    fn get_missile_color(&self, index: usize) -> bool {
        let missile = &self.state.missiles[index];

        if missile.locked {
            return false;
        }

        self.state.electron_beam.x == missile.position
    }

    #[inline]
    fn get_ball_color(&self) -> bool {
        self.state.electron_beam.x == self.state.ball.position
    }

    #[inline]
    fn get_playfield_color(&self) -> Option<TiaColor> {
        let playfield_position = ((self.state.electron_beam.x - HBLANK_LENGTH) / 4) as usize;

        match playfield_position {
            0..20 => {
                if self.state.playfield.data[playfield_position] {
                    if self.state.playfield.score_mode {
                        Some(self.state.players[0].color)
                    } else {
                        Some(self.state.playfield.color)
                    }
                } else {
                    None
                }
            }
            20..40 => {
                let mut data = self.state.playfield.data;

                if self.state.playfield.mirror {
                    data.reverse();
                }

                if data[playfield_position - 20] {
                    if self.state.playfield.score_mode {
                        Some(self.state.players[1].color)
                    } else {
                        Some(self.state.playfield.color)
                    }
                } else {
                    None
                }
            }
            _ => unreachable!(),
        }
    }
}
