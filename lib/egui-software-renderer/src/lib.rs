extern crate std;

use core::ops::RangeInclusive;
use std::collections::HashMap;

use crate::powerof2::PowerOfTwoIter;
use egui::{FullOutput, TextureId};
use fluxemu_graphics::api::software::texture::{
    CopyMode, Texture, TextureImpl, TextureImplMut, TextureViewMut,
};
use fluxemu_range::ContiguousRange;
use multiversion::inherit_target;
use nalgebra::Vector4;
use nalgebra::{Point2, SMatrix, Vector2, Vector3};
use palette::{Srgba, blend::Compose, named::BLACK};
use rustc_hash::FxBuildHasher;

use crate::shapes::{Primitive, Triangle, reduce_shapes};

mod powerof2;
mod shapes;

// NOTE: https://github.com/emilk/egui/pull/2071
//
// ^^ Read that before touching this

#[derive(Debug, Default)]
pub struct Renderer {
    textures: HashMap<TextureId, Texture<Srgba<u8>>, FxBuildHasher>,
}

impl Renderer {
    /// Render to a surface
    #[inline]
    pub fn render<P: From<Srgba<u8>> + Into<Srgba<u8>> + Copy>(
        &mut self,
        context: &egui::Context,
        full_output: FullOutput,
        target_texture: TextureViewMut<P>,
    ) {
        for (new_texture_id, image_delta) in &full_output.textures_delta.set {
            assert!(
                image_delta.is_whole() || self.textures.contains_key(new_texture_id),
                "Texture not found: {new_texture_id:?}"
            );

            if image_delta.is_whole() {
                self.textures.remove(new_texture_id);
            }

            let destination_texture = self.textures.entry(*new_texture_id).or_insert_with(|| {
                let image_size = image_delta.image.size();
                Texture::new(image_size[0], image_size[1], BLACK.into_format().into())
            });

            let source_texture_view = match &image_delta.image {
                egui::ImageData::Color(image) => {
                    let converted_image = image
                        .pixels
                        .clone()
                        .into_iter()
                        .map(|pixel| Srgba::from_components(pixel.to_tuple()))
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

        Self::render_inner(&mut self.textures, context, full_output, target_texture);
    }

    // TODO: Enable neon on arm and v on riscv when such things are stable to query at runtime

    #[inline]
    #[multiversion::multiversion(targets(
        "x86_64+avx512f+avx512dq+fma",
        "x86_64+avx2+fma",
        "x86_64+avx+fma",
        "x86_64+sse4.2",
        "x86+sse2",
        "x86+sse",
        "aarch64+sve2",
        "aarch64+sve",
    ))]
    fn render_inner<P: From<Srgba<u8>> + Into<Srgba<u8>> + Copy>(
        textures: &mut HashMap<TextureId, Texture<Srgba<u8>>, FxBuildHasher>,
        context: &egui::Context,
        full_output: FullOutput,
        mut target_texture: TextureViewMut<P>,
    ) {
        for shape in reduce_shapes(context, full_output.shapes, full_output.pixels_per_point) {
            for primitive in shape.primitives {
                match primitive {
                    Primitive::SolidQuad(solid_quad) => {
                        let min = solid_quad
                            .min
                            .coords
                            .zip_map(&shape.min.coords, |a, b| a.max(b));

                        let max = solid_quad
                            .max
                            .coords
                            .zip_map(&shape.max.coords, |a, b| a.min(b));

                        let min: Point2<_> = Vector2::new(min.x as usize, min.y as usize)
                            .zip_map(&target_texture.size(), |a, b| a.min(b))
                            .into();

                        let max: Point2<_> = Vector2::new(max.x as usize, max.y as usize)
                            .zip_map(&target_texture.size(), |a, b| a.min(b))
                            .into();

                        let mut region = target_texture.view_mut(min.x..max.x, min.y..max.y);
                        region.fill(solid_quad.color.into_format().into());
                    }
                    Primitive::Triangle(triangle) => {
                        let texture = &textures[&shape.texture_id];

                        let target_texture_width = target_texture.width() as f32;
                        let target_texture_height = target_texture.height() as f32;

                        let vertex_x_max = Vector3::new(
                            triangle.v0.position.x,
                            triangle.v1.position.x,
                            triangle.v2.position.x,
                        )
                        .max();
                        let vertex_y_max = Vector3::new(
                            triangle.v0.position.y,
                            triangle.v1.position.y,
                            triangle.v2.position.y,
                        )
                        .max();

                        // Clip the clipping box by the target texture size
                        let clip_x_max = shape.max.x.min(target_texture_width - 1.0);
                        let clip_y_max = shape.max.y.min(target_texture_height - 1.0);

                        // Clip the triangle
                        let triangle_bounding_max = Point2::new(
                            vertex_x_max.min(clip_x_max).floor(),
                            vertex_y_max.min(clip_y_max).floor(),
                        );

                        let vertex_x_min = Vector3::new(
                            triangle.v0.position.x,
                            triangle.v1.position.x,
                            triangle.v2.position.x,
                        )
                        .min();
                        let vertex_y_min = Vector3::new(
                            triangle.v0.position.y,
                            triangle.v1.position.y,
                            triangle.v2.position.y,
                        )
                        .min();

                        // Ensure negative clip values do not exist
                        let clip_x_min = shape.min.x.max(0.0);
                        let clip_y_min = shape.min.y.max(0.0);

                        // Clip the triangle again
                        let triangle_bounding_min = Point2::new(
                            vertex_x_min.max(clip_x_min).ceil(),
                            vertex_y_min.max(clip_y_min).ceil(),
                        );

                        let mut barycentric_coordinates = barycentric_coordinates(
                            // Offset to the center of the pixel
                            triangle_bounding_min + Vector2::from_element(0.5),
                            &triangle,
                        );
                        let mut row_start_barycentric_coordinates = barycentric_coordinates;

                        // Units of which the pixel iteration machine will be advanced incrementally

                        let step_x =
                            Vector3::new(triangle.edge1.y, triangle.edge2.y, triangle.edge0.y)
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

                        let texture_dimensions: Vector2<f32> = texture.size().cast();

                        for y in triangle_bounding_min.y as usize..=triangle_bounding_max.y as usize
                        {
                            // This calculates the enter and exit point of which this particular scanline will be relevant
                            // to the triangle we are drawing

                            let x_enter = (0..3)
                                .map(|index| {
                                    (if step_x[index] > 0.0 {
                                        triangle_bounding_min.x
                                            - row_start_barycentric_coordinates[index]
                                                / step_x[index]
                                    } else {
                                        triangle_bounding_min.x
                                    }) - 0.5
                                })
                                .fold(triangle_bounding_min.x, f32::max)
                                .ceil() as usize;

                            let x_exit = (0..3)
                                .map(|index| {
                                    (if step_x[index] < 0.0 {
                                        triangle_bounding_min.x
                                            - row_start_barycentric_coordinates[index]
                                                / step_x[index]
                                    } else {
                                        triangle_bounding_max.x
                                    }) + 0.5
                                })
                                .fold(triangle_bounding_max.x, f32::min)
                                .floor() as usize;

                            // Advance coordinates
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

                            // This power of two iterator forcing constant run lengths makes very efficient simd code

                            for len in PowerOfTwoIter::<32>::new(x_range.len()) {
                                let target_pixel_row = target_texture
                                    .view_mut(RangeInclusive::from_start_and_length(x, len), y..=y);

                                // Note these functions are perfectly safe, the unsafe is required due to enabling simd features
                                // that are already guarded against by multiversion

                                match len {
                                    32 => unsafe {
                                        pixel_rounds::<32, P>(
                                            target_pixel_row,
                                            texture,
                                            texture_dimensions,
                                            current_uv,
                                            current_color,
                                            step_uv,
                                            step_color,
                                        );
                                    },
                                    16 => unsafe {
                                        pixel_rounds::<16, P>(
                                            target_pixel_row,
                                            texture,
                                            texture_dimensions,
                                            current_uv,
                                            current_color,
                                            step_uv,
                                            step_color,
                                        );
                                    },
                                    8 => unsafe {
                                        pixel_rounds::<8, P>(
                                            target_pixel_row,
                                            texture,
                                            texture_dimensions,
                                            current_uv,
                                            current_color,
                                            step_uv,
                                            step_color,
                                        );
                                    },
                                    4 => unsafe {
                                        pixel_rounds::<4, P>(
                                            target_pixel_row,
                                            texture,
                                            texture_dimensions,
                                            current_uv,
                                            current_color,
                                            step_uv,
                                            step_color,
                                        );
                                    },
                                    2 => unsafe {
                                        pixel_rounds::<2, P>(
                                            target_pixel_row,
                                            texture,
                                            texture_dimensions,
                                            current_uv,
                                            current_color,
                                            step_uv,
                                            step_color,
                                        );
                                    },
                                    1 => unsafe {
                                        pixel_rounds::<1, P>(
                                            target_pixel_row,
                                            texture,
                                            texture_dimensions,
                                            current_uv,
                                            current_color,
                                            step_uv,
                                            step_color,
                                        );
                                    },
                                    _ => {
                                        unreachable!()
                                    }
                                }

                                // Advance everything
                                x += len;
                                barycentric_coordinates += step_x * len as f32;
                                current_uv += step_uv * len as f32;
                                current_color += step_color * len as f32;
                            }

                            row_start_barycentric_coordinates += step_y;
                        }
                    }
                }
            }
        }

        for remove_texture_id in full_output.textures_delta.free {
            tracing::trace!("Freeing egui texture {:?}", remove_texture_id);
            textures.remove(&remove_texture_id);
        }

        // This is written in a way to be very obvious to autovectorizers
        //
        // Currently as it stands, it has very good through output via automatic simd generation

        #[inline]
        #[inherit_target]
        unsafe fn pixel_rounds<const C: usize, P: From<Srgba<u8>> + Into<Srgba<u8>> + Copy>(
            mut target_pixel_row: TextureViewMut<P>,
            texture: &Texture<Srgba<u8>>,
            texture_dimensions: Vector2<f32>,
            current_uv: Vector2<f32>,
            current_color: Srgba<f32>,
            step_uv: Vector2<f32>,
            step_color: Srgba<f32>,
        ) {
            // Assert these dimensions so the compiler doesn't forget
            assert_eq!(target_pixel_row.width(), C);
            assert_eq!(target_pixel_row.height(), 1);

            // Calculate UVs
            let mut interpolated_uvs = SMatrix::<f32, C, 2>::from_element(0.0);
            for index in 0..C {
                let uv = current_uv + (step_uv * index as f32);
                interpolated_uvs.row_mut(index).copy_from(&uv.transpose());
            }

            // Calculate positions within the texture
            let mut texture_positions = SMatrix::<u32, C, 2>::from_element(0);
            for index in 0..C {
                let uv = interpolated_uvs.row(index);

                let texture_position = texture_dimensions
                    .component_mul(&uv.transpose())
                    .zip_map(&Vector2::from_element(0.0), |a, b| a.max(b));

                let pixel_coords =
                    Vector2::<u32>::new(texture_position.x as u32, texture_position.y as u32)
                        .zip_map(
                            &(texture.size().cast() - Vector2::from_element(1)),
                            |a, b| a.min(b),
                        );

                texture_positions
                    .row_mut(index)
                    .copy_from(&pixel_coords.transpose());
            }

            // Gather fetch
            let mut texture_pixels = SMatrix::<f32, C, 4>::from_element(0.0);
            for index in 0..C {
                let texture_position = texture_positions.row(index).transpose();

                let texture_pixel =
                    unsafe { texture.get_unchecked(texture_position.cast()) }.into_format();
                let texture_pixel = Vector4::from_row_slice(texture_pixel.as_ref());

                texture_pixels.set_row(index, &texture_pixel.transpose());
            }

            // Calculate colors
            let mut interpolated_colors = SMatrix::<f32, C, 4>::from_element(0.0);
            for index in 0..C {
                let color = current_color + (step_color * index as f32);
                let color = Vector4::from_row_slice(color.as_ref());

                interpolated_colors.set_row(index, &color.transpose());
            }

            // Blend texture pixels and the colors
            let mut source_pixels = SMatrix::<f32, C, 4>::from_element(0.0);
            for index in 0..C {
                let row = texture_pixels
                    .row(index)
                    .component_mul(&interpolated_colors.row(index));

                source_pixels.set_row(index, &row);
            }

            // Extract the pixels from the destination textures
            let mut destination_pixels = SMatrix::<f32, C, 4>::from_element(0.0);
            for index in 0..C {
                let pixel = target_pixel_row[Point2::new(index, 0)].into().into_format();
                let pixel = Vector4::from_column_slice(pixel.as_ref());

                destination_pixels.set_row(index, &pixel.transpose());
            }

            // Over composite
            for index in 0..C {
                let source = source_pixels.row(index);
                let source: Srgba<f32> = Srgba::from(<[_; 4]>::from(source.transpose()));

                let destination = destination_pixels.row(index);
                let destination: Srgba<f32> = Srgba::from(<[_; 4]>::from(destination.transpose()));

                let output = source.over(destination);

                let output = Vector4::from_row_slice(output.as_ref());
                destination_pixels.set_row(index, &output.transpose());
            }

            // Write back pixels
            for index in 0..C {
                let destination = destination_pixels.row(index);
                let destination: Srgba<f32> = Srgba::from(<[_; 4]>::from(destination.transpose()));

                target_pixel_row[Point2::new(index, 0)] = destination.into_format().into()
            }
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
