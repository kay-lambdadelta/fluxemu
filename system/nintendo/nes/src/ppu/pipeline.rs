use fluxemu_runtime::{
    RuntimeHandle,
    memory::AddressSpace,
    scheduler::{Period, QuantaAllocator},
};
use nalgebra::Point2;

use crate::ppu::{
    ATTRIBUTE_BASE_ADDRESS, BACKGROUND_PALETTE_BASE_ADDRESS, NAMETABLE_BASE_ADDRESS, Ppu,
    SPRITE_PALETTE_BASE_ADDRESS, TOTAL_SCANLINE_LENGTH,
    backend::SupportedGraphicsApi,
    color::PpuColorIndex,
    oam::{CurrentlyRenderingSprite, OamSprite, SpriteEvaluationState},
    region::Region,
    state::VramAddressPointerContents,
};

impl<R: Region, G: SupportedGraphicsApi> Ppu<R, G> {
    #[inline]
    pub(super) fn task(
        &mut self,
        runtime_handle: &RuntimeHandle,
        quanta_allocator: QuantaAllocator<'_, '_>,
    ) {
        let mut ppu_address_space = runtime_handle
            .address_space(self.ppu_address_space)
            .unwrap();

        for timestamp in quanta_allocator {
            if (0..R::VISIBLE_SCANLINES).contains(&self.state.cycle_counter.y) {
                self.handle_visible_scanlines(&mut ppu_address_space, timestamp);
            } else if self.state.cycle_counter.y == R::PRERENDER_SCANLINE {
                self.handle_prerender(&mut ppu_address_space, timestamp);
            }

            if self.state.cycle_counter.y == R::PRERENDER_SCANLINE
                && self.state.cycle_counter.x == 339
                && self.state.odd_frame
                && self.rendering_enabled()
            {
                self.state.cycle_counter = Point2::default();
            } else {
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
    }

    pub(super) fn handle_prerender(
        &mut self,
        ppu_address_space: &mut AddressSpace<'_>,
        timestamp: Period,
    ) {
        if self.state.cycle_counter.x == 1 {
            self.state.oam.sprite_zero_hit = false;
            self.state.oam.overflow = false;
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

        if let 1..=256 = self.state.cycle_counter.x {
            self.drive_background_pipeline(ppu_address_space, timestamp);
        }

        self.increment_vy();

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

        if let 257..=320 = self.state.cycle_counter.x
            && self.rendering_enabled()
        {
            self.state.oam.oam_addr = 0;
        }

        if let 321..=336 = self.state.cycle_counter.x {
            self.drive_background_pipeline(ppu_address_space, timestamp);
        }
    }

    pub(super) fn handle_visible_scanlines(
        &mut self,
        ppu_address_space: &mut AddressSpace<'_>,
        timestamp: Period,
    ) {
        self.process_scanline(ppu_address_space, timestamp, true);
    }

    #[inline]
    pub(super) fn process_scanline(
        &mut self,
        ppu_address_space: &mut AddressSpace<'_>,
        timestamp: Period,
        render: bool,
    ) {
        if self.state.cycle_counter.x == 1 {
            self.state.oam.secondary_address = 0;
            self.state.oam.sprite_zero_in_secondary = false;
            self.state.oam.sprite_evaluation_state = SpriteEvaluationState::Searching;
        }

        if let 1..=256 = self.state.cycle_counter.x {
            let scanline_position_x = self.state.cycle_counter.x - 1;

            let mut sprite_color_index = None;

            let color_relevant_sprite = self.fetch_potential_sprite(scanline_position_x);
            let sprite_behind_background = color_relevant_sprite
                .map(|(sprite, _)| sprite.oam.behind_background)
                .unwrap_or(false);

            if let Some((sprite, color_index)) = color_relevant_sprite {
                sprite_color_index = Some(self.calculate_sprite_color(
                    ppu_address_space,
                    timestamp,
                    sprite.oam,
                    color_index,
                ));
            }

            let tile_pixel = (scanline_position_x % 8) as u8;
            let bit_position = 15 - self.state.background.fine_x_scroll - tile_pixel;

            let high = (self.state.background.pattern_high_shift >> bit_position) & 1;
            let low = (self.state.background.pattern_low_shift >> bit_position) & 1;

            let attribute = (self.state.background.attribute_shift
                >> (30 - (self.state.background.fine_x_scroll + tile_pixel) * 2))
                & 0b11;

            let background_color_bits = (high << 1) | low;
            let is_background_visible = self.state.background.rendering_enabled
                && (self.state.background.show_leftmost_pixels || scanline_position_x >= 8);
            let is_background_opaque = is_background_visible && background_color_bits != 0;

            let background_color_index = if is_background_opaque {
                self.calculate_background_color(
                    ppu_address_space,
                    timestamp,
                    attribute as u8,
                    background_color_bits as u8,
                )
            } else {
                // Backdrop color
                self.calculate_background_color(ppu_address_space, timestamp, 0, 0)
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

            if render {
                let color_index = if self.state.greyscale {
                    color_index & 0b0011_0000
                } else {
                    color_index
                };
                let palette = &self.palette[usize::from(self.state.emphasis_index & 0b111)];
                let clamped_index = (color_index as usize).min(palette.len() - 1);

                self.staging_buffer
                    [Point2::new(scanline_position_x, self.state.cycle_counter.y).cast()] =
                    palette[clamped_index].into();
            }

            self.sprite_zero_check(scanline_position_x, is_background_opaque);

            self.drive_background_pipeline(ppu_address_space, timestamp);
        }

        if self.rendering_enabled() {
            match self.state.cycle_counter.x {
                1..=64 => self.clear_secondary_oam(),
                65..=256 => self.evaluate_sprites(),
                _ => {}
            }
        }

        self.increment_vy();

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
            self.drive_sprite_pipeline(ppu_address_space, timestamp);
        }

        if let 321..=336 = self.state.cycle_counter.x {
            self.drive_background_pipeline(ppu_address_space, timestamp);
        }
    }

    #[inline]
    pub(super) fn rendering_enabled(&self) -> bool {
        self.state.background.rendering_enabled || self.state.oam.rendering_enabled
    }

    #[inline]
    pub(super) fn is_evaluating_sprites(&self) -> bool {
        self.rendering_enabled() && self.state.cycle_counter.y < R::VISIBLE_SCANLINES
    }

    #[inline]
    pub(super) fn is_on_rendering_scanline(&self) -> bool {
        self.state.cycle_counter.y < R::VISIBLE_SCANLINES
            || self.state.cycle_counter.y == R::PRERENDER_SCANLINE
    }

    #[inline]
    pub(super) fn oam_bus_value(&self) -> u8 {
        if self.state.cycle_counter.x > 320 || self.state.cycle_counter.x == 0 {
            self.state.oam.secondary_data[0]
        } else {
            self.state.oam.bus_latch
        }
    }

    #[inline]
    fn increment_vy(&mut self) {
        if self.state.cycle_counter.x == 256 && self.rendering_enabled() {
            self.increment_fine_y();
        }
    }

    #[inline]
    fn increment_fine_y(&mut self) {
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

    #[inline]
    fn increment_coarse_x(&mut self) {
        let mut vram_address_pointer_contents =
            VramAddressPointerContents::from(self.state.vram_address_pointer);

        if vram_address_pointer_contents.coarse.x == 31 {
            vram_address_pointer_contents.coarse.x = 0;
            vram_address_pointer_contents.nametable.x = !vram_address_pointer_contents.nametable.x;
        } else {
            vram_address_pointer_contents.coarse.x += 1;
        }

        self.state.vram_address_pointer = vram_address_pointer_contents.into();
    }

    #[inline]
    fn clear_secondary_oam(&mut self) {
        self.state.oam.bus_latch = 0xff;

        if self.state.cycle_counter.x.is_multiple_of(2) {
            self.state.oam.secondary_data[usize::from(self.state.cycle_counter.x / 2 - 1)] = 0xff;
        }
    }

    #[inline]
    fn sprite_in_range(&self, sprite_y: u8) -> bool {
        let sprite_height = if self.state.oam.sprite_8x16_mode {
            16
        } else {
            8
        };

        self.state.cycle_counter.y.wrapping_sub(u16::from(sprite_y)) < sprite_height
    }

    #[inline]
    fn advance_oam_address(&mut self, amount: u8) -> bool {
        let (oam_addr, wrapped) = self.state.oam.oam_addr.overflowing_add(amount);
        self.state.oam.oam_addr = oam_addr;

        wrapped
    }

    #[inline]
    fn store_secondary_oam(&mut self, value: u8) {
        if let Some(slot) = self
            .state
            .oam
            .secondary_data
            .get_mut(usize::from(self.state.oam.secondary_address))
        {
            *slot = value;
        }
    }

    fn evaluate_sprites(&mut self) {
        if !self.state.cycle_counter.x.is_multiple_of(2) {
            self.state.oam.bus_latch = self.state.oam.data[usize::from(self.state.oam.oam_addr)];
            return;
        }

        let value = self.state.oam.bus_latch;

        match self.state.oam.sprite_evaluation_state {
            SpriteEvaluationState::Searching => {
                self.store_secondary_oam(value);

                if self.sprite_in_range(value) {
                    if self.state.cycle_counter.x == 66 {
                        self.state.oam.sprite_zero_in_secondary = true;
                    }

                    self.state.oam.secondary_address += 1;
                    self.advance_oam_address(1);
                    self.state.oam.sprite_evaluation_state =
                        SpriteEvaluationState::Copying { remaining: 3 };
                } else if self.advance_oam_address(4) {
                    self.state.oam.sprite_evaluation_state = SpriteEvaluationState::Finished;
                }
            }
            SpriteEvaluationState::Copying { remaining } => {
                self.store_secondary_oam(value);
                self.state.oam.secondary_address += 1;
                let wrapped = self.advance_oam_address(1);

                self.state.oam.sprite_evaluation_state = if remaining > 1 {
                    SpriteEvaluationState::Copying {
                        remaining: remaining - 1,
                    }
                } else if wrapped {
                    SpriteEvaluationState::Finished
                } else if self.state.oam.secondary_address >= 32 {
                    SpriteEvaluationState::CheckingOverflow
                } else {
                    SpriteEvaluationState::Searching
                };
            }
            SpriteEvaluationState::CheckingOverflow => {
                if self.sprite_in_range(value) {
                    self.state.oam.overflow = true;
                    self.advance_oam_address(1);
                    self.state.oam.sprite_evaluation_state =
                        SpriteEvaluationState::CopyingOverflowed { remaining: 3 };
                } else {
                    let byte_index = self.state.oam.oam_addr.wrapping_add(1) & 0b11;
                    let wrapped = self.advance_oam_address(4);
                    self.state.oam.oam_addr = (self.state.oam.oam_addr & !0b11) | byte_index;

                    if wrapped {
                        self.state.oam.sprite_evaluation_state = SpriteEvaluationState::Finished;
                    }
                }
            }
            SpriteEvaluationState::CopyingOverflowed { remaining } => {
                self.advance_oam_address(1);

                self.state.oam.sprite_evaluation_state = if remaining > 1 {
                    SpriteEvaluationState::CopyingOverflowed {
                        remaining: remaining - 1,
                    }
                } else {
                    SpriteEvaluationState::Finished
                };
            }
            SpriteEvaluationState::Finished => {
                self.advance_oam_address(4);
            }
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

        if is_background_opaque
            && sprite_zero_hit
            && scanline_position_x != 255
            && self.state.oam.rendering_enabled
            && (self.state.oam.show_leftmost_pixels || scanline_position_x >= 8)
        {
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

    #[inline]
    pub(crate) fn drive_sprite_pipeline(
        &mut self,
        ppu_address_space: &mut AddressSpace<'_>,
        timestamp: Period,
    ) {
        if !self.rendering_enabled() {
            return;
        }

        self.state.oam.oam_addr = 0;

        let position = self.state.cycle_counter.x - 257;
        let sprite_index = usize::from(position / 8);
        let phase = usize::from(position % 8);
        let secondary_offset = sprite_index * 4;

        self.state.oam.bus_latch = self.state.oam.secondary_data[secondary_offset + phase.min(3)];

        if secondary_offset >= usize::from(self.state.oam.secondary_address) {
            return;
        }

        let mut bytes = [0; 4];
        bytes.copy_from_slice(
            &self.state.oam.secondary_data[secondary_offset..secondary_offset + 4],
        );
        let sprite = OamSprite::from_bytes(bytes);

        match phase {
            5 => {
                let address = self.state.oam.calculate_sprite_pattern_address(
                    &sprite,
                    self.state.cycle_counter.y,
                    false,
                );

                self.state.oam.fetched_pattern_table_low = ppu_address_space
                    .read_le_value::<_, false>(address as usize, timestamp)
                    .unwrap();
            }
            7 => {
                let address = self.state.oam.calculate_sprite_pattern_address(
                    &sprite,
                    self.state.cycle_counter.y,
                    true,
                );

                let pattern_table_high = ppu_address_space
                    .read_le_value::<_, false>(address as usize, timestamp)
                    .unwrap();

                self.state
                    .oam
                    .currently_rendering_sprites
                    .push(CurrentlyRenderingSprite {
                        oam: sprite,
                        pattern_table_high,
                        pattern_table_low: self.state.oam.fetched_pattern_table_low,
                        is_sprite_zero: sprite_index == 0
                            && self.state.oam.sprite_zero_in_secondary,
                    })
                    .unwrap();
            }
            _ => {}
        }
    }

    #[inline]
    pub(crate) fn drive_background_pipeline(
        &mut self,
        ppu_address_space: &mut AddressSpace<'_>,
        timestamp: Period,
    ) {
        if !self.rendering_enabled() {
            return;
        }

        match self.state.cycle_counter.x % 8 {
            2 => self.fetch_nametable(ppu_address_space, timestamp),
            4 => self.fetch_attribute(ppu_address_space, timestamp),
            6 => self.fetch_pattern_table_low(ppu_address_space, timestamp),
            0 => self.fetch_pattern_table_high(ppu_address_space, timestamp),
            _ => {}
        }
    }

    #[inline(never)]
    fn fetch_nametable(&mut self, ppu_address_space: &mut AddressSpace<'_>, timestamp: Period) {
        let vram_address_pointer_contents =
            VramAddressPointerContents::from(self.state.vram_address_pointer);

        let nametable_index = (vram_address_pointer_contents.nametable.y as usize) * 2
            + (vram_address_pointer_contents.nametable.x as usize);

        let nametable_base = NAMETABLE_BASE_ADDRESS + (nametable_index * 0x400);

        let address = nametable_base
            + (usize::from(vram_address_pointer_contents.coarse.y) * 32)
            + usize::from(vram_address_pointer_contents.coarse.x);

        self.state.background.fetched_nametable = ppu_address_space
            .read_le_value::<_, false>(address, timestamp)
            .unwrap();
    }

    #[inline(never)]
    fn fetch_attribute(&mut self, ppu_address_space: &mut AddressSpace<'_>, timestamp: Period) {
        let vram_address_pointer_contents =
            VramAddressPointerContents::from(self.state.vram_address_pointer);

        let coarse = vram_address_pointer_contents.coarse.cast::<usize>();
        let nametable_index = (vram_address_pointer_contents.nametable.y as usize) * 2
            + (vram_address_pointer_contents.nametable.x as usize);

        // Attribute table is every 4x4 tiles
        let address = ATTRIBUTE_BASE_ADDRESS
            + (nametable_index * 0x400)
            + (coarse.y / 4) * 8
            + (coarse.x / 4);

        let attribute: u8 = ppu_address_space
            .read_le_value::<_, false>(address, timestamp)
            .unwrap();

        let attribute_quadrant = Point2::new(coarse.x % 4, coarse.y % 4) / 2;
        let shift = (attribute_quadrant.y * 2 + attribute_quadrant.x) * 2;

        self.state.background.fetched_attribute = (attribute >> shift) & 0b11;
    }

    #[inline(never)]
    fn fetch_pattern_table_low(
        &mut self,
        ppu_address_space: &mut AddressSpace<'_>,
        timestamp: Period,
    ) {
        let address = self.background_pattern_address();

        self.state.background.fetched_pattern_table_low = ppu_address_space
            .read_le_value::<_, false>(address as usize, timestamp)
            .unwrap();
    }

    #[inline(never)]
    fn fetch_pattern_table_high(
        &mut self,
        ppu_address_space: &mut AddressSpace<'_>,
        timestamp: Period,
    ) {
        let address = self.background_pattern_address() + 8;

        let pattern_table_high: u8 = ppu_address_space
            .read_le_value::<_, false>(address as usize, timestamp)
            .unwrap();

        self.state.background.pattern_low_shift <<= 8;
        self.state.background.pattern_high_shift <<= 8;
        self.state.background.attribute_shift <<= 16;

        self.state.background.pattern_low_shift |=
            u16::from(self.state.background.fetched_pattern_table_low);
        self.state.background.pattern_high_shift |= u16::from(pattern_table_high);
        self.state.background.attribute_shift |=
            u32::from(self.state.background.fetched_attribute) * 0x5555;

        self.increment_coarse_x();
    }

    #[inline]
    fn background_pattern_address(&self) -> u16 {
        let vram_address_pointer_contents =
            VramAddressPointerContents::from(self.state.vram_address_pointer);

        let pattern_table_base = u16::from(self.state.background.pattern_table_index) * 0x1000;

        pattern_table_base
            + (u16::from(self.state.background.fetched_nametable) * 16)
            + u16::from(vram_address_pointer_contents.fine_y)
    }

    #[inline]
    pub fn calculate_background_color(
        &self,
        ppu_address_space: &mut AddressSpace<'_>,
        timestamp: Period,
        attribute: u8,
        color: u8,
    ) -> PpuColorIndex {
        let color_bits = color & 0b11;

        // Combine into a 4-bit palette index
        let palette_index = color_bits | (attribute << 2);

        ppu_address_space
            .read_le_value::<u8, false>(
                BACKGROUND_PALETTE_BASE_ADDRESS + palette_index as usize,
                timestamp,
            )
            .unwrap()
            & 0b0011_1111
    }

    #[inline]
    pub fn calculate_sprite_color(
        &self,
        ppu_address_space: &mut AddressSpace<'_>,
        timestamp: Period,
        sprite: OamSprite,
        color: u8,
    ) -> PpuColorIndex {
        let color_bits = color & 0b11;

        ppu_address_space
            .read_le_value::<u8, false>(
                SPRITE_PALETTE_BASE_ADDRESS
                    + (usize::from(sprite.palette_index) * 4)
                    + usize::from(color_bits),
                timestamp,
            )
            .unwrap()
            & 0b0011_1111
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
