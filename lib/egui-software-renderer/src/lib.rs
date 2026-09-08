use std::{collections::HashMap, ops::RangeInclusive};

use egui::{FullOutput, TextureId};
use fluxemu_graphics::api::software::texture::{
    AsViewTexture, AsViewTextureMut, CopyMode, OwnedTexture, StorageMut, Texture,
};
use fluxemu_math::{range::ContiguousRange, rectangle::Rectangle};
use nalgebra::{Point2, Vector2};
use palette::{Srgb, Srgba, blend::PreAlpha, named::BLACK};
use rayon::iter::{IndexedParallelIterator, IntoParallelIterator, ParallelIterator};
use rustc_hash::FxBuildHasher;

use crate::geometry::{
    Primitive,
    fill::{fill_quad, fill_triangle},
    reduce::reduce_geometry,
};

mod geometry;
mod powerof2;

// NOTE: https://github.com/emilk/egui/pull/2071
//
// ^^ Read that before touching this

#[derive(Debug, Default)]
pub struct Renderer {
    textures: HashMap<TextureId, OwnedTexture<PreAlpha<Srgb<f32>>>, FxBuildHasher>,
}

impl Renderer {
    /// Render to a surface
    #[inline]
    pub fn render<
        'a,
        P: From<Srgba<u8>> + Into<Srgba<u8>> + Send + Sync + Copy + 'static,
        const BATCH_SIZE: usize,
    >(
        &mut self,
        context: &egui::Context,
        mut full_output: FullOutput,
        mut target_texture: impl AsViewTextureMut<P> + 'a,
    ) {
        assert!(BATCH_SIZE <= 32, "Batch size is too large to be useful");

        self.update_textures(&mut full_output);

        let to_free = full_output.textures_delta.free.clone();
        let target_texture = target_texture.as_view_mut();

        render_inner::<_, BATCH_SIZE>(context, full_output, target_texture, &mut self.textures);

        for remove_texture_id in to_free {
            tracing::trace!("Freeing egui texture {:?}", remove_texture_id);
            self.textures.remove(&remove_texture_id);
        }

        #[inline]
        #[multiversion::multiversion(targets(
            "x86_64+avx512f+avx512dq+avx512bw+avx512vl+fma",
            "x86_64+avx2+fma",
            "x86_64+sse4.1",
            "x86_64+ssse3",
            "x86+sse2",
            "x86+sse",
            "aarch64+sve2",
            "aarch64+sve",
            "aarch64+neon",
        ))]
        fn render_inner<
            P: From<Srgba<u8>> + Into<Srgba<u8>> + Send + Sync + Copy + 'static,
            const BATCH_SIZE: usize,
        >(
            context: &egui::Context,
            full_output: FullOutput,
            mut target_texture: Texture<impl StorageMut<Pixel = P>>,
            textures: &mut HashMap<TextureId, OwnedTexture<PreAlpha<Srgb<f32>>>, FxBuildHasher>,
        ) {
            assert_ne!(target_texture.width(), 0);
            assert_ne!(target_texture.height(), 0);

            let target_texture = target_texture.as_view_mut();

            let shapes: Vec<_> =
                reduce_geometry(context, full_output.shapes, full_output.pixels_per_point)
                    .collect();

            let band_count = rayon::current_num_threads().clamp(1, target_texture.height());
            let bands = target_texture.split_into_bands_mut(band_count);

            let mut cursor = 0;
            let ranges: Vec<_> = bands
                .iter()
                .map(|band| {
                    let range = RangeInclusive::from_start_and_length(cursor, band.height());

                    cursor += band.height();

                    range
                })
                .collect();

            bands
                .into_par_iter()
                .zip(ranges)
                .for_each(|(mut band, band_range)| {
                    let offset = *band_range.start() as f32;

                    for shape in &shapes {
                        let clip_min = Point2::new(shape.rect.min.x, shape.rect.min.y - offset);
                        let clip_max = Point2::new(shape.rect.max.x, shape.rect.max.y - offset);

                        let clip = Rectangle::from_min_and_max(clip_min, clip_max);

                        if !clip.is_valid() || !shape.rect.is_valid() {
                            continue;
                        }

                        for primitive in shape.primitives.iter().copied() {
                            match primitive {
                                Primitive::SolidQuad(mut quad) => {
                                    quad.rectangle.min.y -= offset;
                                    quad.rectangle.max.y -= offset;

                                    fill_quad(clip, quad, band.as_view_mut());
                                }
                                Primitive::Triangle(mut triangle) => {
                                    triangle.v0.position.y -= offset;
                                    triangle.v1.position.y -= offset;
                                    triangle.v2.position.y -= offset;

                                    fill_triangle::<_, BATCH_SIZE>(
                                        clip,
                                        triangle,
                                        textures[&shape.texture_id].as_view(),
                                        band.as_view_mut(),
                                    );
                                }
                            }
                        }
                    }
                });
        }
    }

    fn update_textures(&mut self, full_output: &mut FullOutput) {
        for (new_texture_id, image_delta) in full_output
            .textures_delta
            .set
            .drain()
            .flat_map(|(id, deltas)| deltas.into_iter().map(move |delta| (id, delta)))
        {
            assert!(
                image_delta.is_whole() || self.textures.contains_key(&new_texture_id),
                "Texture not found: {new_texture_id:?}"
            );

            if image_delta.is_whole() {
                self.textures.remove(&new_texture_id);
            }

            let destination_texture = self.textures.entry(new_texture_id).or_insert_with(|| {
                let image_size = image_delta.image.size();

                Texture::from_value(image_size[0], image_size[1], BLACK.into_format().into())
            });

            // Make sure pixel rounds math does not overflow
            assert_ne!(destination_texture.width(), 0);
            assert_ne!(destination_texture.height(), 0);

            // Make sure an absurdly large texture isn't emitted, as we use u32 indexes in pixel_rounds
            assert!(destination_texture.width() <= u32::MAX as usize);
            assert!(destination_texture.height() <= u32::MAX as usize);

            let source_texture_view = match &image_delta.image {
                egui::ImageData::Color(image) => {
                    let converted_image: Vec<_> = image
                        .pixels
                        .clone()
                        .into_iter()
                        .map(|pixel| {
                            Srgba::from_components(pixel.to_tuple())
                                .into_format()
                                .premultiply()
                        })
                        .collect();

                    Texture::from_storage(image.size[0], image.size[1], converted_image)
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
    }
}
