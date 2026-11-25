use std::f32::consts::FRAC_PI_8;

use nalgebra::Vector2;
use serde::{Deserialize, Serialize};
use strum::{EnumDiscriminants, EnumIter};

pub const DIGITAL_THRESHOLD: f32 = 0.5;

// We cannot make this a constant
fn eight_way_sector_edge_cosine() -> f32 {
    (3.0 * FRAC_PI_8).cos()
}

#[derive(Debug, Clone, Copy, PartialEq, EnumDiscriminants)]
#[strum_discriminants(name(InputShape), derive(Hash, PartialOrd, Ord))]
pub enum InputState {
    Button(f32),
    Direction(Vector2<f32>),
}

impl InputState {
    pub const PRESSED: Self = Self::Button(1.0);
    pub const RELEASED: Self = Self::Button(0.0);

    pub fn button(value: f32) -> Self {
        Self::Button(value.clamp(0.0, 1.0))
    }

    pub fn direction(value: Vector2<f32>) -> Self {
        Self::Direction(value.map(|component| component.clamp(-1.0, 1.0)))
    }

    pub fn shape(&self) -> InputShape {
        self.into()
    }

    pub fn as_button(&self) -> Option<f32> {
        match self {
            Self::Button(value) => Some(*value),
            Self::Direction(_) => None,
        }
    }

    pub fn as_direction(&self) -> Option<Vector2<f32>> {
        match self {
            Self::Button(_) => None,
            Self::Direction(value) => Some(*value),
        }
    }

    pub fn is_held(&self) -> bool {
        match self {
            Self::Button(value) => *value >= DIGITAL_THRESHOLD,
            Self::Direction(value) => value.norm() >= DIGITAL_THRESHOLD,
        }
    }

    pub fn part(self, part: DirectionPart) -> Option<Self> {
        let Some(cardinal) = part.unit_vector() else {
            return Some(self);
        };

        let direction = self.as_direction()?;
        let length = direction.norm();

        let is_in_sector = length >= DIGITAL_THRESHOLD
            && direction.dot(&cardinal) >= length * eight_way_sector_edge_cosine();

        Some(Self::button(if is_in_sector { length } else { 0.0 }))
    }
}

impl InputShape {
    pub fn neutral(self) -> InputState {
        match self {
            Self::Button => InputState::RELEASED,
            Self::Direction => InputState::Direction(Vector2::zeros()),
        }
    }
}

#[derive(
    Serialize,
    Deserialize,
    Debug,
    Default,
    Copy,
    Clone,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    EnumIter,
)]
pub enum DirectionPart {
    #[default]
    All,
    Up,
    Down,
    Left,
    Right,
}

impl DirectionPart {
    pub const CARDINALS: [Self; 4] = [Self::Up, Self::Down, Self::Left, Self::Right];

    pub fn unit_vector(self) -> Option<Vector2<f32>> {
        match self {
            Self::All => None,
            Self::Up => Some(Vector2::new(0.0, -1.0)),
            Self::Down => Some(Vector2::new(0.0, 1.0)),
            Self::Left => Some(Vector2::new(-1.0, 0.0)),
            Self::Right => Some(Vector2::new(1.0, 0.0)),
        }
    }
}
