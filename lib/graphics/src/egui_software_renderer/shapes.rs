use std::collections::HashMap;

use egui::{
    Context, TextureId,
    epaint::{ClippedShape, Primitive},
};
use itertools::Itertools;
use nalgebra::{Point2, Vector2};
use palette::Srgba;
use rustc_hash::FxBuildHasher;

use crate::texture::Texture;

const WHITE_UV: Point2<f32> = Point2::new(0.0, 0.0);

#[derive(Copy, Clone, Debug, PartialEq)]
pub struct Vertex {
    pub position: Point2<f32>,
    pub uv: Point2<f32>,
    pub color: Srgba<f32>,
}

impl From<egui::epaint::Vertex> for Vertex {
    fn from(vertex: egui::epaint::Vertex) -> Self {
        Vertex {
            position: Point2::new(vertex.pos.x, vertex.pos.y),
            uv: Point2::new(vertex.uv.x, vertex.uv.y),
            color: Srgba::from_components(vertex.color.to_tuple()).into_format(),
        }
    }
}

#[derive(Copy, Clone, Debug, PartialEq)]
pub struct Triangle<'a> {
    // Vertexes
    pub v0: Vertex,
    pub v1: Vertex,
    pub v2: Vertex,

    // Edges
    pub edge0: Vector2<f32>,
    pub edge1: Vector2<f32>,
    pub edge2: Vector2<f32>,

    pub signed_double_area: f32,
    pub texture: &'a Texture<Srgba<u8>>,
}

impl<'a> Triangle<'a> {
    #[inline]
    fn new(v0: Vertex, v1: Vertex, v2: Vertex, texture: &'a Texture<Srgba<u8>>) -> Option<Self> {
        let edge0 = v0.position - v1.position;
        let edge1 = v1.position - v2.position;
        let edge2 = v2.position - v0.position;

        let signed_double_area = (-edge0).perp(&edge2);

        // Guard against degenerate triangles
        if !signed_double_area.is_finite() || signed_double_area == 0.0 {
            return None;
        }

        Some(Triangle {
            v0,
            v1,
            v2,
            edge0,
            edge1,
            edge2,
            signed_double_area,
            texture,
        })
    }
}

#[derive(Copy, Clone, Debug, PartialEq)]
pub struct SolidQuad {
    pub min: Point2<f32>,
    pub max: Point2<f32>,
    // This is only for single color quads
    pub color: Srgba<f32>,
}

impl SolidQuad {
    #[inline(always)]
    pub fn new_if_eligible([triangle_a, triangle_b]: [[Vertex; 3]; 2]) -> Option<Self> {
        // Ensure this shape has a solid coloring
        let potential_color = triangle_a[0].color;

        // Egui uses the white uv to indicate that something should be colored exclusively by its vertex color
        //
        // Also reject anything if it isn't opaque

        let is_color_and_uv_eligible = potential_color.alpha == 1.0
            && triangle_a
                .into_iter()
                .chain(triangle_b)
                .all(|vertex| vertex.uv == WHITE_UV && vertex.color == potential_color);

        if !is_color_and_uv_eligible {
            // Not solid
            return None;
        }

        // Ensure this shape is actually a quad
        let mut points = [
            triangle_a[0].position,
            triangle_a[1].position,
            triangle_a[2].position,
            triangle_b[0].position,
            triangle_b[1].position,
            triangle_b[2].position,
        ];
        points.sort_unstable_by(|point_a, point_b| {
            point_a
                .x
                .total_cmp(&point_b.x)
                .then(point_a.y.total_cmp(&point_b.y))
        });
        let unique_positions: heapless::Vec<_, 6> = points
            .into_iter()
            .dedup_by(|point_a, point_b| {
                point_a.x.total_cmp(&point_b.x).is_eq() && point_a.y.total_cmp(&point_b.y).is_eq()
            })
            .collect();
        if unique_positions.len() != 4 {
            // Objectively not a quad
            return None;
        }

        // Ensure this is actually a rectangle
        let (min_x, max_x): (f32, f32) = unique_positions
            .iter()
            .fold((f32::INFINITY, f32::NEG_INFINITY), |(min, max), point| {
                (min.min(point.x), max.max(point.x))
            });
        let (min_y, max_y): (f32, f32) = unique_positions
            .iter()
            .fold((f32::INFINITY, f32::NEG_INFINITY), |(min, max), point| {
                (min.min(point.y), max.max(point.y))
            });

        let min = Point2::new(min_x, min_y);
        let max = Point2::new(max_x, max_y);

        // A rectangle has exactly 2 unique x and 2 unique y values
        let points_match_rectangle = unique_positions.iter().all(|point| {
            (point.x == min.x || point.x == max.x) && (point.y == min.y || point.y == max.y)
        });

        if !points_match_rectangle {
            // Not rectangle
            return None;
        }

        Some(SolidQuad {
            min,
            max,
            color: potential_color,
        })
    }
}

pub enum Shape<'a> {
    Triangle(Triangle<'a>),
    SolidQuad(SolidQuad),
}

#[inline(always)]
pub fn emit_shapes<'a>(
    context: &'a Context,
    input_shapes: Vec<ClippedShape>,
    pixels_per_point: f32,
    textures: &'a HashMap<TextureId, Texture<Srgba<u8>>, FxBuildHasher>,
) -> impl Iterator<Item = Shape<'a>> + 'a {
    let mut shapes = Vec::default();

    for clipped_primitive in context.tessellate(input_shapes, pixels_per_point) {
        match clipped_primitive.primitive {
            Primitive::Mesh(mesh) => {
                let mut triangles = mesh
                    .indices
                    .chunks_exact(3)
                    .map(
                        #[inline]
                        |vertex_indexes| {
                            let [mut v0, mut v1, mut v2]: [Vertex; 3] = [
                                mesh.vertices[vertex_indexes[0] as usize].into(),
                                mesh.vertices[vertex_indexes[1] as usize].into(),
                                mesh.vertices[vertex_indexes[2] as usize].into(),
                            ];

                            // Scale for our physical screen dimensions
                            v0.position *= pixels_per_point;
                            v1.position *= pixels_per_point;
                            v2.position *= pixels_per_point;

                            (v0, v1, v2, mesh.texture_id)
                        },
                    )
                    .peekable();

                while let Some((v0, v1, v2, texture_id)) = triangles.next() {
                    if let Some(next_triangle) = triangles.peek()
                        && let Some(solid_quad) = SolidQuad::new_if_eligible([
                            [v0, v1, v2],
                            [next_triangle.0, next_triangle.1, next_triangle.2],
                        ])
                    {
                        triangles.next();

                        shapes.push(Shape::SolidQuad(solid_quad));
                    } else {
                        let texture = &textures[&texture_id];

                        if let Some(triangle) = Triangle::new(v0, v1, v2, texture) {
                            shapes.push(Shape::Triangle(triangle));
                        }
                    }
                }
            }
            Primitive::Callback(_) => {
                unreachable!("Epaint callbacks should not be sent");
            }
        }
    }

    shapes.into_iter()
}
