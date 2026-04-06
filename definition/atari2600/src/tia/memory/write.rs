use fluxemu_definition_mos6502::{Flag, Mos6502, Mos6502Event};
use fluxemu_runtime::{RuntimeApi, event::EventMode, scheduler::Period};
use nalgebra::Point2;

use super::WriteRegisters;
use crate::tia::{
    DelayChangeGraphicPlayer, DelayEnableChangeBall, InputControl, SCANLINE_LENGTH,
    SupportedGraphicsApiTia, Tia, color::TiaColor, region::Region,
};

const PLAYER_RESP_OFFSET: u16 = 3;
const OTHER_RESP_OFFSET: u16 = 2;

impl<R: Region, G: SupportedGraphicsApiTia> Tia<R, G> {
    pub(crate) fn handle_write_register(&mut self, data: u8, address: WriteRegisters) {
        match address {
            WriteRegisters::Vsync => {
                if data & 0b0000_0010 != 0 {
                    self.electron_beam = Point2::new(0, 0);
                    self.cycles_waiting_for_vsync = Some(SCANLINE_LENGTH * 3);
                } else {
                    if let Some(cycles) = self.cycles_waiting_for_vsync
                        && cycles != 0
                    {
                        tracing::warn!("Vsync exited early");
                    }

                    self.cycles_waiting_for_vsync = None;
                }
            }
            WriteRegisters::Vblank => {
                self.vblank_active = data & 0b0000_0010 != 0;

                let bit = if data & 0b1000_0000 != 0 {
                    InputControl::LatchedOrDumped
                } else {
                    InputControl::Normal
                };

                self.input_control[0] = bit;
                self.input_control[1] = bit;
                self.input_control[2] = bit;
                self.input_control[3] = bit;

                let bit = if data & 0b0100_0000 != 0 {
                    InputControl::LatchedOrDumped
                } else {
                    InputControl::Normal
                };

                self.input_control[4] = bit;
                self.input_control[5] = bit;
            }
            WriteRegisters::Wsync => {
                let runtime = RuntimeApi::current();

                // The TIA runs 3 times as fast as the cpu
                let until =
                    Period::from_num(SCANLINE_LENGTH - self.electron_beam.x) / (R::frequency() / 3);

                runtime.schedule_event::<Mos6502>(
                    &self.cpu_path,
                    EventMode::Once,
                    self.timestamp,
                    Mos6502Event::FlagChange {
                        flag: Flag::Rdy,
                        value: false,
                    },
                );
                runtime.schedule_event::<Mos6502>(
                    &self.cpu_path,
                    EventMode::Once,
                    self.timestamp + until,
                    Mos6502Event::FlagChange {
                        flag: Flag::Rdy,
                        value: true,
                    },
                );
            }
            WriteRegisters::Rsync => {
                self.electron_beam.x = 0;
            }
            WriteRegisters::Nusiz0 => {}
            WriteRegisters::Nusiz1 => {}
            WriteRegisters::Colup0 => {
                let color = extract_color(data);

                self.players[0].color = color;
                self.missiles[0].color = color;
            }
            WriteRegisters::Colup1 => {
                let color = extract_color(data);

                self.players[1].color = color;
                self.missiles[1].color = color;
            }
            WriteRegisters::Colupf => {
                let color = extract_color(data);

                self.playfield.color = color;
            }
            WriteRegisters::Colubk => {
                let color = extract_color(data);

                self.background_color = color;
            }
            WriteRegisters::Ctrlpf => {
                self.playfield.mirror = data & 0b0000_0001 != 0;
                self.playfield.score_mode = data & 0b0000_0010 != 0;

                self.high_playfield_ball_priority = data & 0b0000_0100 != 0;

                self.ball.size = 2u8.pow(((data & 0b0011_0000) >> 4) as u32);
            }
            WriteRegisters::Refp0 => {
                self.players[0].mirror = data & 0b0000_1000 != 0;
            }
            WriteRegisters::Refp1 => {
                self.players[1].mirror = data & 0b0000_1000 != 0;
            }
            WriteRegisters::Pf0 => {
                for i in 0..4 {
                    self.playfield.data[i] = data & (1 << i) != 0;
                }
            }
            WriteRegisters::Pf1 => {
                for i in 0..8 {
                    self.playfield.data[4 + (7 - i)] = data & (1 << i) != 0;
                }
            }
            WriteRegisters::Pf2 => {
                for i in 0..8 {
                    self.playfield.data[12 + i] = data & (1 << i) != 0;
                }
            }
            WriteRegisters::Resp0 => {
                self.players[0].position = self.electron_beam.x;
            }
            WriteRegisters::Resp1 => {
                self.players[1].position = self.electron_beam.x;
            }
            WriteRegisters::Resm0 => {
                self.missiles[0].position = self.electron_beam.x;
            }
            WriteRegisters::Resm1 => {
                self.missiles[1].position = self.electron_beam.x;
            }
            WriteRegisters::Resbl => {
                self.ball.position = self.electron_beam.x;
            }
            WriteRegisters::Audc0 => {}
            WriteRegisters::Audc1 => {}
            WriteRegisters::Audf0 => {}
            WriteRegisters::Audf1 => {}
            WriteRegisters::Audv0 => {}
            WriteRegisters::Audv1 => {}
            WriteRegisters::Grp0 => {
                if matches!(
                    self.players[0].delay_change_graphic,
                    DelayChangeGraphicPlayer::Disabled
                ) {
                    self.players[0].graphic = data;
                } else {
                    self.players[0].delay_change_graphic =
                        DelayChangeGraphicPlayer::Enabled(Some(data));
                }

                if let DelayChangeGraphicPlayer::Enabled(Some(graphic)) =
                    self.players[1].delay_change_graphic
                {
                    self.players[1].graphic = graphic;
                    self.players[1].delay_change_graphic = DelayChangeGraphicPlayer::Enabled(None);
                }
            }
            WriteRegisters::Grp1 => {
                if matches!(
                    self.players[1].delay_change_graphic,
                    DelayChangeGraphicPlayer::Disabled
                ) {
                    self.players[1].graphic = data;
                } else {
                    self.players[1].delay_change_graphic =
                        DelayChangeGraphicPlayer::Enabled(Some(data));
                }

                if let DelayChangeGraphicPlayer::Enabled(Some(graphic)) =
                    self.players[0].delay_change_graphic
                {
                    self.players[0].graphic = graphic;
                    self.players[0].delay_change_graphic = DelayChangeGraphicPlayer::Enabled(None);
                }

                if let DelayEnableChangeBall::Enabled(Some(enabled)) = self.ball.delay_enable_change
                {
                    self.ball.enabled = enabled;
                    self.ball.delay_enable_change = DelayEnableChangeBall::Enabled(None);
                }
            }
            WriteRegisters::Enam0 => {
                self.missiles[0].enabled = data & 0b0000_0010 != 0;
            }
            WriteRegisters::Enam1 => {
                self.missiles[1].enabled = data & 0b0000_0010 != 0;
            }
            WriteRegisters::Enabl => {
                if matches!(
                    self.ball.delay_enable_change,
                    DelayEnableChangeBall::Disabled
                ) {
                    self.ball.enabled = data & 0b0000_0010 != 0;
                } else {
                    self.ball.delay_enable_change =
                        DelayEnableChangeBall::Enabled(Some(data & 0b0000_0010 != 0));
                }
            }
            WriteRegisters::Hmp0 => {
                self.players[0].motion = extract_motion(data);
            }
            WriteRegisters::Hmp1 => {
                self.players[1].motion = extract_motion(data);
            }
            WriteRegisters::Hmm0 => {
                self.missiles[0].motion = extract_motion(data);
            }
            WriteRegisters::Hmm1 => {
                self.missiles[1].motion = extract_motion(data);
            }
            WriteRegisters::Hmbl => {
                self.ball.motion = extract_motion(data);
            }
            WriteRegisters::Vdelp0 => {
                if data & 0b000_0001 != 0 {
                    if matches!(
                        self.players[0].delay_change_graphic,
                        DelayChangeGraphicPlayer::Disabled
                    ) {
                        self.players[0].delay_change_graphic =
                            DelayChangeGraphicPlayer::Enabled(None);
                    }
                } else {
                    self.players[0].delay_change_graphic = DelayChangeGraphicPlayer::Disabled;
                }
            }
            WriteRegisters::Vdelp1 => {
                if data & 0b000_0001 != 0 {
                    if matches!(
                        self.players[1].delay_change_graphic,
                        DelayChangeGraphicPlayer::Disabled
                    ) {
                        self.players[1].delay_change_graphic =
                            DelayChangeGraphicPlayer::Enabled(None);
                    }
                } else {
                    self.players[1].delay_change_graphic = DelayChangeGraphicPlayer::Disabled;
                }
            }
            WriteRegisters::Vdelbl => {
                if data & 0b000_0001 != 0 {
                    if matches!(
                        self.ball.delay_enable_change,
                        DelayEnableChangeBall::Disabled
                    ) {
                        self.ball.delay_enable_change = DelayEnableChangeBall::Enabled(None);
                    }
                } else {
                    self.ball.delay_enable_change = DelayEnableChangeBall::Disabled;
                }
            }
            WriteRegisters::Resmp0 => {
                self.missiles[0].locked = data & 0b000_0010 != 0;
            }
            WriteRegisters::Resmp1 => {
                self.missiles[1].locked = data & 0b000_0010 != 0;
            }
            WriteRegisters::Hmove => {
                for player in &mut self.players {
                    player.position = player
                        .position
                        .wrapping_add_signed(i16::from(player.motion));
                }

                for missile in &mut self.missiles {
                    missile.position = missile
                        .position
                        .wrapping_add_signed(i16::from(missile.motion));
                }

                self.ball.position = self
                    .ball
                    .position
                    .wrapping_add_signed(i16::from(self.ball.motion));
            }
            WriteRegisters::Hmclr => {
                self.players[0].motion = 0;
                self.players[1].motion = 0;
                self.missiles[0].motion = 0;
                self.missiles[1].motion = 0;
                self.ball.motion = 0;
            }
            WriteRegisters::Cxclr => {
                self.collision_matrix.clear();
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
