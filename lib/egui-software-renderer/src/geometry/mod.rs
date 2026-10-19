use std::sync::Arc;

use egui::TextureId;
use fluxemu_math::rectangle::Rectangle;
use nalgebra::{Point2, Vector2};
use palette::Srgba;

use crate::{
    callback::Callback,
    intermediate::{Channel, Intermediate},
};

pub mod fill;
pub mod reduce;

#[derive(Copy, Clone, Debug, PartialEq)]
pub struct Vertex<T: Intermediate> {
    pub position: Point2<T>,
    pub uv: Point2<T>,
    pub color: Srgba<Channel<T>>,
}

#[derive(Copy, Clone, Debug, PartialEq)]
pub struct Triangle<I: Intermediate> {
    // Vertexes
    pub v0: Vertex<I>,
    pub v1: Vertex<I>,
    pub v2: Vertex<I>,

    // Edges
    pub edge0: Vector2<I>,
    pub edge1: Vector2<I>,
    pub edge2: Vector2<I>,

    // Double area
    pub signed_double_area: I,
}

#[derive(Copy, Clone, Debug, PartialEq)]
pub struct SolidQuad<I: Intermediate> {
    pub rectangle: Rectangle<I>,
    // This is only for single color quads
    pub color: Srgba<Channel<I>>,
}

#[derive(Debug)]
pub struct Group<P: 'static, I: Intermediate> {
    pub rect: Rectangle<I>,
    pub primitives: Vec<Primitive<P, I>>,
}

#[derive(Debug)]
pub enum Primitive<P: 'static, I: Intermediate> {
    Triangle {
        shape: Triangle<I>,
        texture_id: TextureId,
    },
    SolidQuad(SolidQuad<I>),
    Callback(Arc<Callback<P>>),
}
