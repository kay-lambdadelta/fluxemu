use egui::{Context, epaint::ClippedShape};
use itertools::Itertools;
use nalgebra::Point2;
use palette::Srgba;

use crate::geometry::{Primitive, Shape, SolidQuad, Triangle, Vertex};

const WHITE_UV: Point2<f32> = Point2::new(0.0, 0.0);

#[inline]
pub fn reduce_geometry(
    context: &Context,
    input_shapes: Vec<ClippedShape>,
    pixels_per_point: f32,
) -> impl Iterator<Item = Shape> {
    let mut shapes = Vec::default();

    for clipped_primitive in context.tessellate(input_shapes, pixels_per_point) {
        match clipped_primitive.primitive {
            egui::epaint::Primitive::Mesh(mesh) => {
                let mut primitives = Vec::default();

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

                            (v0, v1, v2)
                        },
                    )
                    .peekable();

                while let Some((v0, v1, v2)) = triangles.next() {
                    if let Some((next_v0, next_v1, next_v2)) = triangles.peek()
                        && let Some(solid_quad) = SolidQuad::new_if_eligible([
                            [v0, v1, v2],
                            [*next_v0, *next_v1, *next_v2],
                        ])
                    {
                        triangles.next();

                        primitives.push(Primitive::SolidQuad(solid_quad));
                    } else {
                        if let Some(triangle) = Triangle::new(v0, v1, v2) {
                            primitives.push(Primitive::Triangle(triangle));
                        }
                    }
                }

                shapes.push(Shape {
                    min: Point2::new(
                        clipped_primitive.clip_rect.min.x,
                        clipped_primitive.clip_rect.min.y,
                    ) * pixels_per_point,
                    max: Point2::new(
                        clipped_primitive.clip_rect.max.x,
                        clipped_primitive.clip_rect.max.y,
                    ) * pixels_per_point,
                    texture_id: mesh.texture_id,
                    primitives,
                });
            }
            egui::epaint::Primitive::Callback(_) => {
                unreachable!("Epaint callbacks should not be sent");
            }
        }
    }

    shapes.into_iter()
}

impl From<egui::epaint::Vertex> for Vertex {
    #[inline]
    fn from(vertex: egui::epaint::Vertex) -> Self {
        Vertex {
            position: Point2::new(vertex.pos.x, vertex.pos.y),
            uv: Point2::new(vertex.uv.x, vertex.uv.y),
            color: Srgba::from_components(vertex.color.to_tuple()).into_format(),
        }
    }
}

impl Triangle {
    #[inline]
    fn new(v0: Vertex, v1: Vertex, v2: Vertex) -> Option<Self> {
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
        })
    }
}

impl SolidQuad {
    #[inline]
    fn new_if_eligible([triangle_a, triangle_b]: [[Vertex; 3]; 2]) -> Option<Self> {
        // Ensure this shape has a solid coloring
        let potential_color = triangle_a[0].color;

        // Egui uses the white uv to indicate that something should be colored exclusively by its vertex color
        //
        // This is an indicator that this triangle is solidly colored (if the alpha is 1.0)
        let is_color_and_uv_eligible = potential_color.alpha == 1.0
            && triangle_a
                .into_iter()
                .chain(triangle_b)
                .all(|vertex| vertex.uv == WHITE_UV && vertex.color == potential_color);

        if !is_color_and_uv_eligible {
            // Not solid
            return None;
        }

        // If this shape is a quad, it will have exactly 4 unique positions from the contributed vertexes

        let mut points = [
            triangle_a[0].position,
            triangle_a[1].position,
            triangle_a[2].position,
            triangle_b[0].position,
            triangle_b[1].position,
            triangle_b[2].position,
        ];

        // Ensure they are sorted so deduplication works correctly
        points.sort_unstable_by(
            #[inline]
            |point_a, point_b| {
                point_a
                    .x
                    .total_cmp(&point_b.x)
                    .then(point_a.y.total_cmp(&point_b.y))
            },
        );

        let unique_positions: heapless::Vec<_, 6> = points
            .into_iter()
            .dedup_by(
                #[inline]
                |point_a, point_b| {
                    point_a.x.total_cmp(&point_b.x).is_eq()
                        && point_a.y.total_cmp(&point_b.y).is_eq()
                },
            )
            .collect();

        if unique_positions.len() != 4 {
            // Objectively not a quad
            return None;
        }

        // A rectangle will have 2 unique x values and 2 unique y values for its position
        let (min_x, max_x): (f32, f32) = unique_positions.iter().fold(
            (f32::INFINITY, f32::NEG_INFINITY),
            #[inline]
            |(min, max), point| (min.min(point.x), max.max(point.x)),
        );
        let (min_y, max_y): (f32, f32) = unique_positions.iter().fold(
            (f32::INFINITY, f32::NEG_INFINITY),
            #[inline]
            |(min, max), point| (min.min(point.y), max.max(point.y)),
        );

        let min = Point2::new(min_x, min_y);
        let max = Point2::new(max_x, max_y);

        let points_match_rectangle = unique_positions.iter().all(
            #[inline]
            |point| {
                (point.x == min.x || point.x == max.x) && (point.y == min.y || point.y == max.y)
            },
        );

        if !points_match_rectangle {
            // Not axis aligned
            return None;
        }

        Some(SolidQuad {
            min,
            max,
            color: potential_color,
        })
    }
}
