use egui::TextureId;
use nalgebra::{Point2, Vector2};
use palette::Srgba;

pub mod fill;
pub mod reduce;

#[derive(Copy, Clone, Debug, PartialEq)]
pub struct Vertex {
    pub position: Point2<f32>,
    pub uv: Point2<f32>,
    pub color: Srgba<f32>,
}

#[derive(Copy, Clone, Debug, PartialEq)]
pub struct Triangle {
    // Vertexes
    pub v0: Vertex,
    pub v1: Vertex,
    pub v2: Vertex,

    // Edges
    pub edge0: Vector2<f32>,
    pub edge1: Vector2<f32>,
    pub edge2: Vector2<f32>,

    // Double area
    pub signed_double_area: f32,
}

#[derive(Copy, Clone, Debug, PartialEq)]
pub struct SolidQuad {
    pub min: Point2<f32>,
    pub max: Point2<f32>,
    // This is only for single color quads
    pub color: Srgba<f32>,
}

#[derive(Debug)]
pub struct Shape {
    pub min: Point2<f32>,
    pub max: Point2<f32>,
    pub texture_id: TextureId,
    pub primitives: Vec<Primitive>,
}

#[derive(Debug, Clone, Copy)]
pub enum Primitive {
    Triangle(Triangle),
    SolidQuad(SolidQuad),
}
