use nalgebra::{Point2, Vector2};
use serde::{Deserialize, Serialize};
use serde_with::{Bytes, serde_as};

#[derive(Serialize, Deserialize, Debug, Default, Clone, Copy)]
pub struct OamSprite {
    pub position: Point2<u8>,
    pub tile_index: u8,
    pub palette_index: u8,
    pub behind_background: bool,
    pub flip: Vector2<bool>,
}

impl OamSprite {
    pub fn from_bytes(bytes: [u8; 4]) -> Self {
        let position = Point2::new(bytes[3], bytes[0]);

        let tile_index = bytes[1];
        let attributes = bytes[2];

        let palette_index = attributes & 0b0000_0011;
        let priority = attributes & 0b0010_0000 != 0;

        let flip = Vector2::new(attributes & 0b0100_0000 != 0, attributes & 0b1000_0000 != 0);

        OamSprite {
            position,
            tile_index,
            palette_index,
            behind_background: priority,
            flip,
        }
    }

    #[allow(unused)]
    pub fn to_bytes(self) -> [u8; 4] {
        let mut bytes = [0; 4];
        bytes[0] = self.position.y;
        bytes[1] = self.tile_index;
        bytes[3] = self.position.x;
        bytes[2] = (self.palette_index & 0b0000_0011)
            | (self.behind_background as u8) << 5
            | (self.flip.x as u8) << 6
            | (self.flip.y as u8) << 7;
        bytes
    }
}

#[derive(Serialize, Deserialize, Debug, Default, Clone, Copy)]
pub struct CurrentlyRenderingSprite {
    pub oam: OamSprite,
    pub pattern_table_low: u8,
    pub pattern_table_high: u8,
}

#[serde_as]
#[derive(Serialize, Deserialize, Debug, Clone, Copy)]
pub enum SpriteEvaluationState {
    InspectingY,
    Evaluating { sprite_y: u8 },
}

#[serde_as]
#[derive(Serialize, Deserialize, Debug)]
pub struct OamState {
    /// Sprite data buffer 4 byte sprites and 64 of them
    #[serde_as(as = "Bytes")]
    pub data: [u8; 256],
    /// Secondary data buffer that is filled with sprite evaluation
    pub secondary_data: heapless::Vec<OamSprite, 8>,
    /// Internal feature of this emulator filled with sprites post fetching
    pub currently_rendering_sprites: heapless::Vec<CurrentlyRenderingSprite, 8>,
    pub oam_addr: u8,
    pub sprite_evaluation_state: SpriteEvaluationState,
    pub show_sprites_leftmost_pixels: bool,
    pub sprite_8x8_pattern_table_index: u8,
    pub rendering_enabled: bool,
    pub awaiting_memory_access: bool,
}
