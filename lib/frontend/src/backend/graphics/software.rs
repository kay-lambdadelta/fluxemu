use std::collections::HashMap;

use egui::{FullOutput, TextureId};
use fluxemu_runtime::graphics::software::{Texture, TextureImpl, TextureImplMut, TextureViewMut};
use nalgebra::{Point2, Scalar, Vector2, Vector3, Vector4};
use palette::{
    Srgba, WithAlpha,
    blend::Compose,
    cast::{ComponentOrder, Packed},
    named::BLACK,
};
use rustc_hash::FxBuildHasher;

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
struct Triangle {
    // Vertexes
    v0: Vertex,
    v1: Vertex,
    v2: Vertex,

    // Edges
    edge0: Vector2<f32>,
    edge1: Vector2<f32>,
    edge2: Vector2<f32>,

    signed_double_area: f32,
}

impl Triangle {
    #[inline]
    fn new(v0: Vertex, v1: Vertex, v2: Vertex) -> Self {
        let edge0 = v0.position - v1.position;
        let edge2 = v2.position - v0.position;

        let signed_double_area = (-edge0).perp(&edge2);

        Triangle {
            v0,
            v1,
            v2,
            edge0,
            edge1: v1.position - v2.position,
            edge2,
            signed_double_area,
        }
    }
}

#[derive(Debug, Default)]
pub struct EguiRenderer {
    textures: HashMap<TextureId, Texture<Srgba<f32>>, FxBuildHasher>,
}

impl EguiRenderer {
    /// Render to a surface given the pixel order
    pub fn render<P: ComponentOrder<Srgba<u8>, [u8; 4]> + Scalar + Send + Sync>(
        &mut self,
        context: &egui::Context,
        full_output: FullOutput,
        mut target_texture: TextureViewMut<Packed<P, [u8; 4]>>,
    ) {
        for (new_texture_id, image_delta) in full_output.textures_delta.set {
            assert!(
                image_delta.is_whole() || self.textures.contains_key(&new_texture_id),
                "Texture not found: {new_texture_id:?}"
            );

            if image_delta.is_whole() {
                self.textures.remove(&new_texture_id);
            }

            let destination_texture = self.textures.entry(new_texture_id).or_insert_with(|| {
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

            destination_texture.copy_from(
                &source_texture_view,
                texture_update_offset.x
                    ..(texture_update_offset.x + source_texture_view.width())
                        .min(destination_texture.width()),
                texture_update_offset.y
                    ..(texture_update_offset.y + source_texture_view.height())
                        .min(destination_texture.height()),
            );
        }

        let render_buffer_dimensions: Vector2<f32> = target_texture.size().cast();

        for (triangle, texture_id) in context
            .tessellate(full_output.shapes, full_output.pixels_per_point)
            .iter()
            .flat_map(|shape| match &shape.primitive {
                egui::epaint::Primitive::Mesh(mesh) => {
                    mesh.indices.chunks_exact(3).map(|vertex_indexes| {
                        let [mut v0, mut v1, mut v2]: [Vertex; 3] = [
                            mesh.vertices[vertex_indexes[0] as usize].into(),
                            mesh.vertices[vertex_indexes[1] as usize].into(),
                            mesh.vertices[vertex_indexes[2] as usize].into(),
                        ];

                        // Scale for our physical screen dimensions
                        v0.position *= full_output.pixels_per_point;
                        v1.position *= full_output.pixels_per_point;
                        v2.position *= full_output.pixels_per_point;

                        (Triangle::new(v0, v1, v2), mesh.texture_id)
                    })
                }
                egui::epaint::Primitive::Callback(_) => {
                    unreachable!("Epaint callbacks should not be sent");
                }
            })
        {
            let texture = self.textures.get(&texture_id).unwrap();

            let triangle_bounding_max = Point2::new(
                Vector3::new(
                    triangle.v0.position.x,
                    triangle.v1.position.x,
                    triangle.v2.position.x,
                )
                .max()
                .min(render_buffer_dimensions.x - 1.0) as usize,
                Vector3::new(
                    triangle.v0.position.y,
                    triangle.v1.position.y,
                    triangle.v2.position.y,
                )
                .max()
                .min(render_buffer_dimensions.y - 1.0) as usize,
            );

            let triangle_bounding_min = Point2::new(
                Vector4::new(
                    triangle.v0.position.x,
                    triangle.v1.position.x,
                    triangle.v2.position.x,
                    triangle_bounding_max.x as f32,
                )
                .min()
                .max(0.0) as usize,
                Vector4::new(
                    triangle.v0.position.y,
                    triangle.v1.position.y,
                    triangle.v2.position.y,
                    triangle_bounding_max.y as f32,
                )
                .min()
                .max(0.0) as usize,
            );

            let mut barycentric_coordinates = barycentric_coordinates(
                triangle_bounding_min.cast() + Vector2::from_element(0.5),
                &triangle,
            );
            let mut row_start_barycentric_coordinates = barycentric_coordinates;

            let step_x = Vector3::new(
                triangle.v1.position.y - triangle.v2.position.y,
                triangle.v2.position.y - triangle.v0.position.y,
                triangle.v0.position.y - triangle.v1.position.y,
            ) / triangle.signed_double_area;

            let step_y = Vector3::new(
                triangle.v2.position.x - triangle.v1.position.x,
                triangle.v0.position.x - triangle.v2.position.x,
                triangle.v1.position.x - triangle.v0.position.x,
            ) / triangle.signed_double_area;

            let texture_dimensions: Vector2<f32> = texture.size().cast();

            for y in triangle_bounding_min.y..=triangle_bounding_max.y {
                barycentric_coordinates = row_start_barycentric_coordinates;

                for x in triangle_bounding_min.x..=triangle_bounding_max.x {
                    let position = Point2::new(x, y);

                    let source_pixel = if is_inside_triangle(barycentric_coordinates) {
                        let interpolated_color = triangle.v0.color * barycentric_coordinates.x
                            + triangle.v1.color * barycentric_coordinates.y
                            + triangle.v2.color * barycentric_coordinates.z;

                        let interpolated_uv = triangle.v0.uv.coords * barycentric_coordinates.x
                            + triangle.v1.uv.coords * barycentric_coordinates.y
                            + triangle.v2.uv.coords * barycentric_coordinates.z;

                        let pixel_coords: Point2<_> = Point2::new(
                            (texture_dimensions.x * interpolated_uv.x) as usize,
                            (texture_dimensions.y * interpolated_uv.y) as usize,
                        )
                        .coords
                        .zip_map(&(texture.size() - Vector2::from_element(1)), |a, b| {
                            a.min(b)
                        })
                        .into();

                        let pixel = texture[pixel_coords];

                        interpolated_color * pixel
                    } else {
                        BLACK.with_alpha(0.0).into_format()
                    };

                    let destination_pixel = &mut target_texture[position];

                    *destination_pixel = Packed::pack(Srgba::from_format(
                        source_pixel.over(destination_pixel.unpack().into_format()),
                    ));

                    barycentric_coordinates += step_x;
                }

                row_start_barycentric_coordinates += step_y;
            }
        }

        for remove_texture_id in full_output.textures_delta.free {
            tracing::trace!("Freeing egui texture {:?}", remove_texture_id);
            self.textures.remove(&remove_texture_id);
        }
    }
}

#[inline]
fn barycentric_coordinates(point: Point2<f32>, triangle: &Triangle) -> Vector3<f32> {
    let v0p = triangle.v0.position - point;
    let v1p = triangle.v1.position - point;
    let v2p = triangle.v2.position - point;

    let area = Vector3::new(v1p.perp(&v2p), v2p.perp(&v0p), v0p.perp(&v1p));

    if triangle.signed_double_area.abs() < f32::EPSILON {
        return Vector3::default();
    }

    area / triangle.signed_double_area
}

// Winding order of egui is undefined
#[inline]
fn is_inside_triangle(coords: Vector3<f32>) -> bool {
    coords.into_iter().all(|&val| val >= 0.0) || coords.into_iter().all(|&val| val <= 0.0)
}
