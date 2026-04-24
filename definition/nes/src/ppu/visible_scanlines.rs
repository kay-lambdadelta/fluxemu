use std::ops::RangeInclusive;

use fluxemu_range::ContiguousRange;
use fluxemu_runtime::{memory::AddressSpace, scheduler::Period};
use nalgebra::Point2;

use crate::ppu::{
    Ppu,
    backend::SupportedGraphicsApiPpu,
    oam::{CurrentlyRenderingSprite, OamSprite, SpriteEvaluationState},
    region::Region,
    state::VramAddressPointerContents,
};

impl<R: Region, G: SupportedGraphicsApiPpu> Ppu<R, G> {
    pub(super) fn handle_visible_scanlines(
        &mut self,
        ppu_address_space: &mut AddressSpace<'_>,
        timestamp: Period,
    ) {
        if self.state.cycle_counter.x == 1 {
            // Technically the NES does it over 64 cycles
            self.state.oam.secondary_data.clear();
            self.state.oam.sprite_zero_in_secondary = false;
        }

        if let 1..=256 = self.state.cycle_counter.x {
            let scanline_position_x = self.state.cycle_counter.x - 1;

            let mut sprite_color_index = None;

            let color_relevant_sprite = self.fetch_potential_sprite(scanline_position_x);
            let sprite_behind_background = color_relevant_sprite
                .map(|(sprite, _)| sprite.oam.behind_background)
                .unwrap_or(false);

            if let Some((sprite, color_index)) = color_relevant_sprite {
                sprite_color_index = Some(self.state.calculate_sprite_color::<R>(
                    ppu_address_space,
                    timestamp,
                    sprite.oam,
                    color_index,
                ));
            }

            let bit_position =
                15 - self.state.background.fine_x_scroll - self.state.background.tile_pixel;

            let high = (self.state.background.pattern_high_shift >> bit_position) & 1;
            let low = (self.state.background.pattern_low_shift >> bit_position) & 1;

            let attribute = (self.state.background.attribute_shift
                >> (30
                    - (self.state.background.fine_x_scroll + self.state.background.tile_pixel)
                        * 2))
                & 0b11;

            self.state.background.tile_pixel += 1;
            if self.state.background.tile_pixel == 8 {
                self.state.background.tile_pixel = 0;
            }

            let background_color_bits = (high << 1) | low;
            let is_background_visible = self.state.background.rendering_enabled
                && (self.state.background.show_leftmost_pixels || scanline_position_x >= 8);
            let is_background_opaque = is_background_visible && background_color_bits != 0;

            let background_color_index = if is_background_opaque {
                self.state.calculate_background_color::<R>(
                    ppu_address_space,
                    timestamp,
                    attribute as u8,
                    background_color_bits as u8,
                )
            } else {
                // Backdrop color
                self.state
                    .calculate_background_color::<R>(ppu_address_space, timestamp, 0, 0)
            };

            let is_sprite_visible = self.state.oam.rendering_enabled
                && (self.state.oam.show_leftmost_pixels || scanline_position_x >= 8);
            let is_sprite_opaque = is_sprite_visible && sprite_color_index.is_some();

            let color_index =
                if is_sprite_opaque && (!sprite_behind_background || !is_background_opaque) {
                    sprite_color_index.unwrap()
                } else if is_background_opaque {
                    background_color_index
                } else if is_sprite_opaque && sprite_behind_background {
                    sprite_color_index.unwrap()
                } else {
                    background_color_index
                };

            self.staging_buffer
                [Point2::new(scanline_position_x, self.state.cycle_counter.y).cast()] = color_index;

            self.sprite_zero_check(scanline_position_x, is_background_opaque);
            self.state
                .drive_background_pipeline::<R>(ppu_address_space, timestamp);
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
                        let sprite_height: u16 = if self.state.oam.sprite_8x16_mode {
                            16
                        } else {
                            8
                        };

                        if (u16::from(sprite_y)..u16::from(sprite_y) + sprite_height)
                            .contains(&self.state.cycle_counter.y)
                        {
                            let mut bytes = [0; 4];
                            bytes.copy_from_slice(
                                &self.state.oam.data[RangeInclusive::from_start_and_length(
                                    oam_data_index as usize,
                                    4,
                                )],
                            );

                            let sprite = OamSprite::from_bytes(bytes);
                            let index = (oam_data_index / 4) as u8;

                            if index == 0 {
                                self.state.oam.sprite_zero_in_secondary = true;
                            }

                            if self.state.oam.secondary_data.push(sprite).is_err() {
                                // TODO: Handle sprite overflow flag
                            }
                        }

                        self.state.oam.sprite_evaluation_state = SpriteEvaluationState::InspectingY;
                    }
                }
            }
        }

        if self.state.cycle_counter.x == 256
            && (self.state.background.rendering_enabled || self.state.oam.rendering_enabled)
        {
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

            if self.state.background.rendering_enabled || self.state.oam.rendering_enabled {
                let t = VramAddressPointerContents::from(self.state.shadow_vram_address_pointer);
                let mut v = VramAddressPointerContents::from(self.state.vram_address_pointer);

                v.nametable.x = t.nametable.x;
                v.coarse.x = t.coarse.x;

                self.state.vram_address_pointer = u16::from(v);
            }
        }

        if let 257..=320 = self.state.cycle_counter.x {
            self.state
                .drive_sprite_pipeline::<R>(ppu_address_space, timestamp);
        }

        if let 321..=336 = self.state.cycle_counter.x {
            self.state
                .drive_background_pipeline::<R>(ppu_address_space, timestamp);
        }
    }

    #[inline]
    fn sprite_zero_check(&mut self, scanline_position_x: u16, is_background_opaque: bool) {
        let sprite_zero_hit = self
            .state
            .oam
            .currently_rendering_sprites
            .first()
            .filter(|sprite| sprite.is_sprite_zero)
            .and_then(|sprite| {
                let color_index = calculate_sprite_color_index(scanline_position_x, sprite)?;

                if color_index != 0 { Some(()) } else { None }
            })
            .is_some();

        if is_background_opaque && sprite_zero_hit && scanline_position_x != 255 {
            self.state.oam.sprite_zero_hit = true;
        }
    }

    fn fetch_potential_sprite(
        &mut self,
        scanline_position_x: u16,
    ) -> Option<(CurrentlyRenderingSprite, u8)> {
        self.state
            .oam
            .currently_rendering_sprites
            .iter()
            .rev()
            .copied()
            .find_map(|sprite| {
                let color_index = calculate_sprite_color_index(scanline_position_x, &sprite)?;

                if color_index != 0 {
                    Some((sprite, color_index))
                } else {
                    None
                }
            })
    }
}

#[inline]
fn calculate_sprite_color_index(
    scanline_position_x: u16,
    sprite: &CurrentlyRenderingSprite,
) -> Option<u8> {
    let in_sprite_position = scanline_position_x.checked_sub(u16::from(sprite.oam.position.x))?;

    if in_sprite_position >= 8 {
        return None;
    }

    let in_sprite_position = if !sprite.oam.flip.x {
        in_sprite_position
    } else {
        7 - in_sprite_position
    };

    let low = (sprite.pattern_table_low >> (7 - in_sprite_position)) & 1;
    let high = (sprite.pattern_table_high >> (7 - in_sprite_position)) & 1;

    let color_index = (high << 1) | low;

    Some(color_index)
}
