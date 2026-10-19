use std::cmp::Ordering;

use egui::{Context, PaintCallback, epaint::ClippedShape};
use fluxemu_math::rectangle::Rectangle;
use itertools::Itertools;
use nalgebra::Point2;
use palette::Srgba;

use crate::{
    callback::Callback,
    geometry::{Group, Primitive, SolidQuad, Triangle, Vertex},
    intermediate::{self, Intermediate, from_f32},
};

#[inline]
pub fn reduce_geometry<P: 'static, I: Intermediate>(
    context: &Context,
    input_shapes: Vec<ClippedShape>,
    pixels_per_point: f32,
) -> impl Iterator<Item = Group<P, I>> {
    let mut shapes: Vec<Group<_, _>> = Vec::default();

    for clipped_primitive in context.tessellate(input_shapes, pixels_per_point) {
        let group = match clipped_primitive.primitive {
            egui::epaint::Primitive::Mesh(mesh) => {
                let mut primitives = Vec::default();

                let mut triangles = mesh
                    .indices
                    .as_chunks::<3>()
                    .0
                    .iter()
                    .map(
                        #[inline]
                        |vertex_indexes| {
                            let [v0, v1, v2]: [Vertex<I>; _] = vertex_indexes.map(|index| {
                                Vertex::new(mesh.vertices[index as usize], pixels_per_point)
                            });

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
                            primitives.push(Primitive::Triangle {
                                shape: triangle,
                                texture_id: mesh.texture_id,
                            });
                        }
                    }
                }

                let rect = scaled_rectangle(clipped_primitive.clip_rect, pixels_per_point);

                Group { rect, primitives }
            }
            egui::epaint::Primitive::Callback(PaintCallback { rect, callback }) => {
                let Ok(callback) = callback.downcast::<Callback<P>>() else {
                    tracing::error!("Wrong callback type");
                    continue;
                };

                let full = scaled_rectangle::<I>(rect, pixels_per_point);
                let clip = scaled_rectangle(clipped_primitive.clip_rect, pixels_per_point);

                let visible_min = intermediate::component_max(full.min, clip.min);
                let visible_max = intermediate::component_min(full.max, clip.max);

                let visible = Rectangle::from_min_and_max(visible_min, visible_max);

                if !visible.is_valid() {
                    continue;
                }

                Group {
                    rect: visible,
                    primitives: vec![Primitive::Callback(callback)],
                }
            }
        };

        let is_mergeable = |group: &Group<P, I>| {
            !matches!(group.primitives.as_slice(), [Primitive::Callback { .. }])
        };

        if let Some(last) = shapes.last_mut()
            && is_mergeable(last)
            && is_mergeable(&group)
            && last.rect == group.rect
        {
            last.primitives.extend(group.primitives);
        } else {
            shapes.push(group);
        }
    }

    shapes.into_iter()
}

#[inline]
fn scaled_rectangle<T: Intermediate>(rect: egui::Rect, pixels_per_point: f32) -> Rectangle<T> {
    Rectangle::from_min_and_max(
        Point2::new(
            from_f32(rect.min.x * pixels_per_point),
            from_f32(rect.min.y * pixels_per_point),
        ),
        Point2::new(
            from_f32(rect.max.x * pixels_per_point),
            from_f32(rect.max.y * pixels_per_point),
        ),
    )
}

impl<T: Intermediate> Vertex<T> {
    #[inline]
    fn new(vertex: egui::epaint::Vertex, pixels_per_point: f32) -> Self {
        Vertex {
            // Scale for our physical screen dimensions
            position: Point2::new(
                from_f32(vertex.pos.x * pixels_per_point),
                from_f32(vertex.pos.y * pixels_per_point),
            ),
            uv: Point2::new(from_f32(vertex.uv.x), from_f32(vertex.uv.y)),
            color: Srgba::from_components(vertex.color.to_tuple()).into_format(),
        }
    }
}

impl<T: Intermediate> Triangle<T> {
    #[inline]
    fn new(v0: Vertex<T>, v1: Vertex<T>, v2: Vertex<T>) -> Option<Self> {
        let edge0 = v0.position - v1.position;
        let edge1 = v1.position - v2.position;
        let edge2 = v2.position - v0.position;

        let signed_double_area = (-edge0).perp(&edge2);

        // Guard against degenerate triangles
        if !intermediate::is_finite(signed_double_area) || signed_double_area.is_zero() {
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

impl<T: Intermediate> SolidQuad<T> {
    #[inline]
    fn new_if_eligible([triangle_a, triangle_b]: [[Vertex<T>; 3]; 2]) -> Option<Self> {
        // Ensure this shape has a solid coloring
        let potential_color = triangle_a[0].color;

        // Egui uses the white uv to indicate that something should be colored exclusively by its vertex color
        //
        // This is an indicator that this triangle is solidly colored (if the alpha is 1.0)
        let white_uv = Point2::new(T::zero(), T::zero());
        let is_color_and_uv_eligible = potential_color.alpha.0.is_one()
            && triangle_a
                .into_iter()
                .chain(triangle_b)
                .all(|vertex| vertex.uv == white_uv && vertex.color == potential_color);

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
                    .partial_cmp(&point_b.x)
                    .unwrap_or(Ordering::Equal)
                    .then(point_a.y.partial_cmp(&point_b.y).unwrap_or(Ordering::Equal))
            },
        );

        let unique_positions: heapless::Vec<_, 6> = points
            .into_iter()
            .dedup_by(
                #[inline]
                |point_a, point_b| point_a == point_b,
            )
            .collect();

        if unique_positions.len() != 4 {
            // Objectively not a quad
            return None;
        }

        // A rectangle will have 2 unique x values and 2 unique y values for its position
        let (min_x, max_x) = unique_positions.iter().fold(
            (T::max_value(), T::min_value()),
            #[inline]
            |(min, max), point| {
                (
                    intermediate::min(min, point.x),
                    intermediate::max(max, point.x),
                )
            },
        );
        let (min_y, max_y) = unique_positions.iter().fold(
            (T::max_value(), T::min_value()),
            #[inline]
            |(min, max), point| {
                (
                    intermediate::min(min, point.y),
                    intermediate::max(max, point.y),
                )
            },
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
            rectangle: Rectangle::from_min_and_max(min, max),
            color: potential_color,
        })
    }
}
