use std::{collections::HashMap, ops::RangeInclusive};

use egui::{FullOutput, TextureId};
use fluxemu_range::ContiguousRange;
use nalgebra::{Point2, Vector2, Vector3, Vector4};
use palette::{Srgba, named::BLACK};
use pixel_rounds::pixel_rounds;
use rustc_hash::FxBuildHasher;

use crate::{
    egui_software_renderer::powerof2::PowerOfTwoIter,
    texture::{CopyMode, Texture, TextureImpl, TextureImplMut, TextureViewMut},
};

mod pixel_rounds;
mod powerof2;

// NOTE: https://github.com/emilk/egui/pull/2071
//
// ^^ Read that before touching this

#[derive(Copy, Clone, Debug, PartialEq)]
struct Vertex {
    position: Point2<f32>,
    uv: Point2<f32>,
    color: Srgba<f32>,
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
struct Triangle<'a> {
    // Vertexes
    v0: Vertex,
    v1: Vertex,
    v2: Vertex,

    // Edges
    edge0: Vector2<f32>,
    edge1: Vector2<f32>,
    edge2: Vector2<f32>,

    signed_double_area: f32,
    texture: &'a Texture<Srgba<f32>>,
}

impl<'a> Triangle<'a> {
    #[inline]
    fn new(v0: Vertex, v1: Vertex, v2: Vertex, texture: &'a Texture<Srgba<f32>>) -> Self {
        let edge0 = v0.position - v1.position;
        let edge1 = v1.position - v2.position;
        let edge2 = v2.position - v0.position;

        let signed_double_area = (-edge0).perp(&edge2);

        Triangle {
            v0,
            v1,
            v2,
            edge0,
            edge1,
            edge2,
            signed_double_area,
            texture,
        }
    }
}

#[derive(Debug, Default)]
pub struct Renderer {
    textures: HashMap<TextureId, Texture<Srgba<f32>>, FxBuildHasher>,
}

impl Renderer {
    /// Render to a surface given the pixel order
    #[inline(never)]
    pub fn render<P: From<Srgba<u8>> + Into<Srgba<u8>> + Copy>(
        &mut self,
        context: &egui::Context,
        full_output: FullOutput,
        target_texture: TextureViewMut<P>,
    ) {
        Self::render_inner(&mut self.textures, context, full_output, target_texture);
    }

    #[inline]
    #[multiversion::multiversion(targets(
        "x86_64+avx512f+avx512dq+fma",
        "x86_64+avx2+fma",
        "x86_64+avx+fma",
        "x86_64+sse4.2",
        "aarch64+sve",
    ))]
    fn render_inner<P: From<Srgba<u8>> + Into<Srgba<u8>> + Copy>(
        textures: &mut HashMap<TextureId, Texture<Srgba<f32>>, FxBuildHasher>,
        context: &egui::Context,
        full_output: FullOutput,
        mut target_texture: TextureViewMut<P>,
    ) {
        for (new_texture_id, image_delta) in full_output.textures_delta.set {
            assert!(
                image_delta.is_whole() || textures.contains_key(&new_texture_id),
                "Texture not found: {new_texture_id:?}"
            );

            if image_delta.is_whole() {
                textures.remove(&new_texture_id);
            }

            let destination_texture = textures.entry(new_texture_id).or_insert_with(|| {
                let image_size = image_delta.image.size();
                Texture::new(image_size[0], image_size[1], BLACK.into_format().into())
            });

            let source_texture_view: Texture<Srgba<f32>> = match &image_delta.image {
                egui::ImageData::Color(image) => {
                    let converted_image = image
                        .pixels
                        .clone()
                        .into_iter()
                        .map(|pixel| Srgba::from_components(pixel.to_tuple()).into_format())
                        .collect();

                    Texture::from_vec(image.size[0], image.size[1], converted_image)
                }
            };

            let texture_update_offset = Vector2::from(image_delta.pos.unwrap_or([0, 0]));

            destination_texture
                .view_mut(
                    texture_update_offset.x
                        ..(texture_update_offset.x + source_texture_view.width()),
                    texture_update_offset.y
                        ..(texture_update_offset.y + source_texture_view.height()),
                )
                .copy_from(&source_texture_view, CopyMode::Nearest);
        }

        let render_buffer_dimensions: Vector2<f32> = target_texture.size().cast();

        let primitives = context.tessellate(full_output.shapes, full_output.pixels_per_point);
        let triangles = primitives.iter().flat_map(|shape| match &shape.primitive {
            egui::epaint::Primitive::Mesh(mesh) => {
                mesh.indices
                    .chunks_exact(3)
                    .map(|vertex_indexes| {
                        let [mut v0, mut v1, mut v2]: [Vertex; 3] = [
                            mesh.vertices[vertex_indexes[0] as usize].into(),
                            mesh.vertices[vertex_indexes[1] as usize].into(),
                            mesh.vertices[vertex_indexes[2] as usize].into(),
                        ];

                        // Scale for our physical screen dimensions
                        v0.position *= full_output.pixels_per_point;
                        v1.position *= full_output.pixels_per_point;
                        v2.position *= full_output.pixels_per_point;

                        let texture = textures.get(&mesh.texture_id).unwrap();

                        Triangle::new(v0, v1, v2, texture)
                    })
                    .filter(|triangle| triangle.signed_double_area.abs() >= f32::EPSILON)
            }
            egui::epaint::Primitive::Callback(_) => {
                unreachable!("Epaint callbacks should not be sent");
            }
        });

        for triangle in triangles {
            let triangle_bounding_max = Point2::new(
                Vector3::new(
                    triangle.v0.position.x,
                    triangle.v1.position.x,
                    triangle.v2.position.x,
                )
                .max()
                .min(render_buffer_dimensions.x - 1.0)
                .floor(),
                Vector3::new(
                    triangle.v0.position.y,
                    triangle.v1.position.y,
                    triangle.v2.position.y,
                )
                .max()
                .min(render_buffer_dimensions.y - 1.0)
                .floor(),
            );

            let triangle_bounding_min = Point2::new(
                Vector4::new(
                    triangle.v0.position.x,
                    triangle.v1.position.x,
                    triangle.v2.position.x,
                    triangle_bounding_max.x,
                )
                .min()
                .max(0.0)
                .ceil(),
                Vector4::new(
                    triangle.v0.position.y,
                    triangle.v1.position.y,
                    triangle.v2.position.y,
                    triangle_bounding_max.y,
                )
                .min()
                .max(0.0)
                .ceil(),
            );

            let mut barycentric_coordinates = barycentric_coordinates(
                triangle_bounding_min + Vector2::from_element(0.5),
                &triangle,
            );
            let mut row_start_barycentric_coordinates = barycentric_coordinates;

            let step_x = Vector3::new(triangle.edge1.y, triangle.edge2.y, triangle.edge0.y)
                / triangle.signed_double_area;

            let step_y = Vector3::new(
                triangle.v2.position.x - triangle.v1.position.x,
                triangle.v0.position.x - triangle.v2.position.x,
                triangle.v1.position.x - triangle.v0.position.x,
            ) / triangle.signed_double_area;

            let step_uv = Vector2::new(
                step_x.x * triangle.v0.uv.x
                    + step_x.y * triangle.v1.uv.x
                    + step_x.z * triangle.v2.uv.x,
                step_x.x * triangle.v0.uv.y
                    + step_x.y * triangle.v1.uv.y
                    + step_x.z * triangle.v2.uv.y,
            );

            let step_color = Srgba::new(
                step_x.dot(&Vector3::new(
                    triangle.v0.color.red,
                    triangle.v1.color.red,
                    triangle.v2.color.red,
                )),
                step_x.dot(&Vector3::new(
                    triangle.v0.color.green,
                    triangle.v1.color.green,
                    triangle.v2.color.green,
                )),
                step_x.dot(&Vector3::new(
                    triangle.v0.color.blue,
                    triangle.v1.color.blue,
                    triangle.v2.color.blue,
                )),
                step_x.dot(&Vector3::new(
                    triangle.v0.color.alpha,
                    triangle.v1.color.alpha,
                    triangle.v2.color.alpha,
                )),
            );

            let texture_dimensions: Vector2<f32> = triangle.texture.size().cast();

            for y in triangle_bounding_min.y as usize..=triangle_bounding_max.y as usize {
                let x_enter = (0..3)
                    .map(|i| {
                        if step_x[i] > 0.0 {
                            triangle_bounding_min.x
                                - row_start_barycentric_coordinates[i] / step_x[i]
                        } else {
                            triangle_bounding_min.x
                        }
                    })
                    .fold(triangle_bounding_min.x, f32::max)
                    .ceil() as usize;

                let x_exit = (0..3)
                    .map(|i| {
                        if step_x[i] < 0.0 {
                            triangle_bounding_min.x
                                - row_start_barycentric_coordinates[i] / step_x[i]
                        } else {
                            triangle_bounding_max.x
                        }
                    })
                    .fold(triangle_bounding_max.x, f32::min)
                    .ceil() as usize;

                barycentric_coordinates = row_start_barycentric_coordinates
                    + step_x * (x_enter as f32 - triangle_bounding_min.x);

                let mut current_uv = triangle.v0.uv.coords * barycentric_coordinates.x
                    + triangle.v1.uv.coords * barycentric_coordinates.y
                    + triangle.v2.uv.coords * barycentric_coordinates.z;

                let mut current_color = triangle.v0.color * barycentric_coordinates.x
                    + triangle.v1.color * barycentric_coordinates.y
                    + triangle.v2.color * barycentric_coordinates.z;

                let x_range = x_enter..=x_exit;
                let mut x = *x_range.start();

                for len in PowerOfTwoIter::<16>::new(x_range.len()) {
                    let target_pixel_row = target_texture
                        .view_mut(RangeInclusive::from_start_and_length(x, len), y..=y);

                    match len {
                        16 => {
                            pixel_rounds::<16, P>(
                                target_pixel_row,
                                &triangle,
                                texture_dimensions,
                                &mut current_uv,
                                &mut current_color,
                                step_uv,
                                step_color,
                            );
                        }
                        8 => {
                            pixel_rounds::<8, P>(
                                target_pixel_row,
                                &triangle,
                                texture_dimensions,
                                &mut current_uv,
                                &mut current_color,
                                step_uv,
                                step_color,
                            );
                        }
                        4 => {
                            pixel_rounds::<4, P>(
                                target_pixel_row,
                                &triangle,
                                texture_dimensions,
                                &mut current_uv,
                                &mut current_color,
                                step_uv,
                                step_color,
                            );
                        }
                        2 => {
                            pixel_rounds::<2, P>(
                                target_pixel_row,
                                &triangle,
                                texture_dimensions,
                                &mut current_uv,
                                &mut current_color,
                                step_uv,
                                step_color,
                            );
                        }
                        1 => {
                            pixel_rounds::<1, P>(
                                target_pixel_row,
                                &triangle,
                                texture_dimensions,
                                &mut current_uv,
                                &mut current_color,
                                step_uv,
                                step_color,
                            );
                        }
                        _ => {
                            unreachable!()
                        }
                    }

                    x += len;
                    barycentric_coordinates += step_x * len as f32;
                }

                row_start_barycentric_coordinates += step_y;
            }
        }

        for remove_texture_id in full_output.textures_delta.free {
            tracing::trace!("Freeing egui texture {:?}", remove_texture_id);
            textures.remove(&remove_texture_id);
        }
    }
}

#[inline]
fn barycentric_coordinates(point: Point2<f32>, triangle: &Triangle) -> Vector3<f32> {
    let v0p = triangle.v0.position - point;
    let v1p = triangle.v1.position - point;
    let v2p = triangle.v2.position - point;

    let area = Vector3::new(v1p.perp(&v2p), v2p.perp(&v0p), v0p.perp(&v1p));

    area / triangle.signed_double_area
}
