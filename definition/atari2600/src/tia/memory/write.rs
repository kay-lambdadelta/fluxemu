use fluxemu_definition_mos6502::{Mos6502, Mos6502Event, Pin};
use fluxemu_runtime::{ComponentRuntimeApi, event::EventMode, scheduler::Period};
use nalgebra::Point2;

use super::WriteRegisters;
use crate::tia::{
    DelayChangeGraphicPlayer, DelayEnableChangeBall, InputControl, SCANLINE_LENGTH,
    SupportedGraphicsApiTia, Tia, color::TiaColor, region::Region,
};

impl<R: Region, G: SupportedGraphicsApiTia> Tia<R, G> {
    pub(crate) fn handle_write_register(&mut self, data: u8, address: WriteRegisters) {
        match address {
            WriteRegisters::Vsync => {
                if data & 0b0000_0010 != 0 {
                    self.state.electron_beam = Point2::new(0, 0);
                    self.state.cycles_waiting_for_vsync = Some(SCANLINE_LENGTH * 3);
                } else {
                    if let Some(cycles) = self.state.cycles_waiting_for_vsync
                        && cycles != 0
                    {
                        tracing::warn!("Vsync exited early");
                    }

                    self.state.cycles_waiting_for_vsync = None;
                }
            }
            WriteRegisters::Vblank => {
                self.state.vblank_active = data & 0b0000_0010 != 0;

                let bit = if data & 0b1000_0000 != 0 {
                    InputControl::LatchedOrDumped
                } else {
                    InputControl::Normal
                };

                self.state.input_control[0] = bit;
                self.state.input_control[1] = bit;
                self.state.input_control[2] = bit;
                self.state.input_control[3] = bit;

                let bit = if data & 0b0100_0000 != 0 {
                    InputControl::LatchedOrDumped
                } else {
                    InputControl::Normal
                };

                self.state.input_control[4] = bit;
                self.state.input_control[5] = bit;
            }
            WriteRegisters::Wsync => {
                let runtime = ComponentRuntimeApi::current(&self.path);
                let timestamp = runtime.current_timestamp();

                // The TIA runs 3 times as fast as the cpu
                let until = Period::from_num(SCANLINE_LENGTH - self.state.electron_beam.x)
                    / (R::frequency() / 3);

                runtime.schedule_event::<Mos6502>(
                    &self.cpu_path,
                    EventMode::Once,
                    timestamp,
                    Mos6502Event::FlagChange {
                        pin: Pin::Rdy,
                        value: false,
                    },
                );
                runtime.schedule_event::<Mos6502>(
                    &self.cpu_path,
                    EventMode::Once,
                    timestamp + until,
                    Mos6502Event::FlagChange {
                        pin: Pin::Rdy,
                        value: true,
                    },
                );
            }
            WriteRegisters::Rsync => {
                self.state.electron_beam.x = 0;
            }
            WriteRegisters::Nusiz0 => {}
            WriteRegisters::Nusiz1 => {}
            WriteRegisters::Colup0 => {
                let color = extract_color(data);

                self.state.players[0].color = color;
                self.state.missiles[0].color = color;
            }
            WriteRegisters::Colup1 => {
                let color = extract_color(data);

                self.state.players[1].color = color;
                self.state.missiles[1].color = color;
            }
            WriteRegisters::Colupf => {
                let color = extract_color(data);

                self.state.playfield.color = color;
            }
            WriteRegisters::Colubk => {
                let color = extract_color(data);

                self.state.background_color = color;
            }
            WriteRegisters::Ctrlpf => {
                self.state.playfield.mirror = data & 0b0000_0001 != 0;
                self.state.playfield.score_mode = data & 0b0000_0010 != 0;

                self.state.high_playfield_ball_priority = data & 0b0000_0100 != 0;

                self.state.ball.size = 2u8.pow(((data & 0b0011_0000) >> 4) as u32);
            }
            WriteRegisters::Refp0 => {
                self.state.players[0].mirror = data & 0b0000_1000 != 0;
            }
            WriteRegisters::Refp1 => {
                self.state.players[1].mirror = data & 0b0000_1000 != 0;
            }
            WriteRegisters::Pf0 => {
                for i in 0..4 {
                    self.state.playfield.data[i] = data & (1 << i) != 0;
                }
            }
            WriteRegisters::Pf1 => {
                for i in 0..8 {
                    self.state.playfield.data[4 + (7 - i)] = data & (1 << i) != 0;
                }
            }
            WriteRegisters::Pf2 => {
                for i in 0..8 {
                    self.state.playfield.data[12 + i] = data & (1 << i) != 0;
                }
            }
            WriteRegisters::Resp0 => {
                self.state.players[0].position = self.state.electron_beam.x;
            }
            WriteRegisters::Resp1 => {
                self.state.players[1].position = self.state.electron_beam.x;
            }
            WriteRegisters::Resm0 => {
                self.state.missiles[0].position = self.state.electron_beam.x;
            }
            WriteRegisters::Resm1 => {
                self.state.missiles[1].position = self.state.electron_beam.x;
            }
            WriteRegisters::Resbl => {
                self.state.ball.position = self.state.electron_beam.x;
            }
            WriteRegisters::Audc0 => {}
            WriteRegisters::Audc1 => {}
            WriteRegisters::Audf0 => {}
            WriteRegisters::Audf1 => {}
            WriteRegisters::Audv0 => {}
            WriteRegisters::Audv1 => {}
            WriteRegisters::Grp0 => {
                if matches!(
                    self.state.players[0].delay_change_graphic,
                    DelayChangeGraphicPlayer::Disabled
                ) {
                    self.state.players[0].graphic = data;
                } else {
                    self.state.players[0].delay_change_graphic =
                        DelayChangeGraphicPlayer::Enabled(Some(data));
                }

                if let DelayChangeGraphicPlayer::Enabled(Some(graphic)) =
                    self.state.players[1].delay_change_graphic
                {
                    self.state.players[1].graphic = graphic;
                    self.state.players[1].delay_change_graphic =
                        DelayChangeGraphicPlayer::Enabled(None);
                }
            }
            WriteRegisters::Grp1 => {
                if matches!(
                    self.state.players[1].delay_change_graphic,
                    DelayChangeGraphicPlayer::Disabled
                ) {
                    self.state.players[1].graphic = data;
                } else {
                    self.state.players[1].delay_change_graphic =
                        DelayChangeGraphicPlayer::Enabled(Some(data));
                }

                if let DelayChangeGraphicPlayer::Enabled(Some(graphic)) =
                    self.state.players[0].delay_change_graphic
                {
                    self.state.players[0].graphic = graphic;
                    self.state.players[0].delay_change_graphic =
                        DelayChangeGraphicPlayer::Enabled(None);
                }

                if let DelayEnableChangeBall::Enabled(Some(enabled)) =
                    self.state.ball.delay_enable_change
                {
                    self.state.ball.enabled = enabled;
                    self.state.ball.delay_enable_change = DelayEnableChangeBall::Enabled(None);
                }
            }
            WriteRegisters::Enam0 => {
                self.state.missiles[0].enabled = data & 0b0000_0010 != 0;
            }
            WriteRegisters::Enam1 => {
                self.state.missiles[1].enabled = data & 0b0000_0010 != 0;
            }
            WriteRegisters::Enabl => {
                if matches!(
                    self.state.ball.delay_enable_change,
                    DelayEnableChangeBall::Disabled
                ) {
                    self.state.ball.enabled = data & 0b0000_0010 != 0;
                } else {
                    self.state.ball.delay_enable_change =
                        DelayEnableChangeBall::Enabled(Some(data & 0b0000_0010 != 0));
                }
            }
            WriteRegisters::Hmp0 => {
                self.state.players[0].motion = extract_motion(data);
            }
            WriteRegisters::Hmp1 => {
                self.state.players[1].motion = extract_motion(data);
            }
            WriteRegisters::Hmm0 => {
                self.state.missiles[0].motion = extract_motion(data);
            }
            WriteRegisters::Hmm1 => {
                self.state.missiles[1].motion = extract_motion(data);
            }
            WriteRegisters::Hmbl => {
                self.state.ball.motion = extract_motion(data);
            }
            WriteRegisters::Vdelp0 => {
                if data & 0b000_0001 != 0 {
                    if matches!(
                        self.state.players[0].delay_change_graphic,
                        DelayChangeGraphicPlayer::Disabled
                    ) {
                        self.state.players[0].delay_change_graphic =
                            DelayChangeGraphicPlayer::Enabled(None);
                    }
                } else {
                    self.state.players[0].delay_change_graphic = DelayChangeGraphicPlayer::Disabled;
                }
            }
            WriteRegisters::Vdelp1 => {
                if data & 0b000_0001 != 0 {
                    if matches!(
                        self.state.players[1].delay_change_graphic,
                        DelayChangeGraphicPlayer::Disabled
                    ) {
                        self.state.players[1].delay_change_graphic =
                            DelayChangeGraphicPlayer::Enabled(None);
                    }
                } else {
                    self.state.players[1].delay_change_graphic = DelayChangeGraphicPlayer::Disabled;
                }
            }
            WriteRegisters::Vdelbl => {
                if data & 0b000_0001 != 0 {
                    if matches!(
                        self.state.ball.delay_enable_change,
                        DelayEnableChangeBall::Disabled
                    ) {
                        self.state.ball.delay_enable_change = DelayEnableChangeBall::Enabled(None);
                    }
                } else {
                    self.state.ball.delay_enable_change = DelayEnableChangeBall::Disabled;
                }
            }
            WriteRegisters::Resmp0 => {
                self.state.missiles[0].locked = data & 0b000_0010 != 0;
            }
            WriteRegisters::Resmp1 => {
                self.state.missiles[1].locked = data & 0b000_0010 != 0;
            }
            WriteRegisters::Hmove => {
                for player in &mut self.state.players {
                    player.position = player
                        .position
                        .wrapping_add_signed(i16::from(player.motion));
                }

                for missile in &mut self.state.missiles {
                    missile.position = missile
                        .position
                        .wrapping_add_signed(i16::from(missile.motion));
                }

                self.state.ball.position = self
                    .state
                    .ball
                    .position
                    .wrapping_add_signed(i16::from(self.state.ball.motion));
            }
            WriteRegisters::Hmclr => {
                self.state.players[0].motion = 0;
                self.state.players[1].motion = 0;
                self.state.missiles[0].motion = 0;
                self.state.missiles[1].motion = 0;
                self.state.ball.motion = 0;
            }
            WriteRegisters::Cxclr => {
                self.state.collision_matrix.clear();
            }
        }
    }
}

#[inline]
fn extract_color(data: u8) -> TiaColor {
    let luminance = (data & 0b0000_1110) >> 1;
    let hue = (data & 0b1111_0000) >> 4;

    TiaColor { luminance, hue }
}

#[inline]
fn extract_motion(data: u8) -> i8 {
    let raw = (data & 0b1111_0000) >> 4;
    ((raw as i8) << 4) >> 4
}
