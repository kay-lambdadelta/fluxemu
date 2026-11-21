use std::{collections::HashMap, fmt::Debug, ops::RangeInclusive};

use egui::{Color32, FullOutput, TextureId};
use fluxemu_graphics::texture::{
    AsViewTexture, AsViewTextureMut, CopyMode, OwnedTexture, RefMutTexture, StorageMut, Texture,
};
use fluxemu_math::{range::ContiguousRange, rectangle::Rectangle};
use nalgebra::{Point2, Vector2};
use palette::{Srgb, Srgba, blend::PreAlpha, named::BLACK};
use rayon::iter::{IndexedParallelIterator, IntoParallelIterator, ParallelIterator};
use rustc_hash::FxBuildHasher;

use crate::{
    callback::CallbackInfo,
    geometry::{
        Group, Primitive,
        fill::{fill_quad, fill_triangle},
        reduce::reduce_geometry,
    },
    intermediate::Channel,
};

pub mod callback;
mod geometry;
mod intermediate;
mod powerof2;

pub use intermediate::Intermediate;

// NOTE: https://github.com/emilk/egui/pull/2071
//
// ^^ Read that before touching this

type TextureMap<T> = HashMap<TextureId, StoredTexture<T>, FxBuildHasher>;

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub struct RendererConfig {
    pub batch_size: usize,
    pub alpha_only_font: bool,
}

impl Default for RendererConfig {
    fn default() -> Self {
        Self {
            // Benchmarks say that a batch size of 16 is ideal across several low and mid power machines
            //
            // As far as throughput for realistic UI goes at the very least
            //
            // This is suggested by benchmarks on a i5-1245U and a RK3566T
            batch_size: 16,
            alpha_only_font: false,
        }
    }
}

#[derive(Debug)]
pub struct Renderer<T: Intermediate = f32> {
    config: RendererConfig,
    textures: TextureMap<T>,
}

impl<T: Intermediate> Default for Renderer<T> {
    fn default() -> Self {
        Self::new(RendererConfig::default())
    }
}

impl<T: Intermediate> Renderer<T> {
    pub fn new(config: RendererConfig) -> Self {
        assert!(
            config.batch_size.is_power_of_two(),
            "Batch size must be a power of two"
        );
        assert!(
            config.batch_size <= 32,
            "Batch size is too large to be useful"
        );

        Self {
            config,
            textures: HashMap::default(),
        }
    }

    pub fn config(&self) -> RendererConfig {
        self.config
    }

    /// Get the max texture size this instance of the renderer can represent
    pub fn max_texture_size(&self) -> u32 {
        T::max_int().to_u32().unwrap_or(u32::MAX)
    }

    /// Render to a surface
    #[inline]
    pub fn render<'a, P: From<Srgba<u8>> + Into<Srgba<u8>> + Send + Sync + Copy + 'static>(
        &mut self,
        context: &egui::Context,
        mut full_output: FullOutput,
        mut target_texture: impl AsViewTextureMut<P> + 'a,
    ) {
        self.update_textures(&mut full_output);

        let to_free = full_output.textures_delta.free.clone();
        let target_texture = target_texture.as_view_mut();

        render_inner(
            context,
            full_output,
            target_texture,
            &self.textures,
            self.config.batch_size,
        );

        for remove_texture_id in to_free {
            tracing::trace!("Freeing egui texture {:?}", remove_texture_id);
            self.textures.remove(&remove_texture_id);
        }
    }

    fn update_textures(&mut self, full_output: &mut FullOutput) {
        let max_texture_size = self.max_texture_size() as usize;

        for (texture_id, image_delta) in full_output
            .textures_delta
            .set
            .drain()
            .flat_map(|(id, deltas)| deltas.into_iter().map(move |delta| (id, delta)))
        {
            assert!(
                image_delta.is_whole() || self.textures.contains_key(&texture_id),
                "Texture not found: {texture_id:?}"
            );

            if image_delta.is_whole() {
                self.textures.remove(&texture_id);
            }

            // Managed texture 0 is always the font atlas
            let alpha_only = self.config.alpha_only_font && texture_id == TextureId::default();

            let destination_texture = self.textures.entry(texture_id).or_insert_with(|| {
                let [width, height] = image_delta.image.size();

                if alpha_only {
                    StoredTexture::Alpha(Texture::from_value(width, height, 0))
                } else {
                    StoredTexture::Color(Texture::from_value(
                        width,
                        height,
                        BLACK.into_format().into(),
                    ))
                }
            });

            // Make sure pixel rounds math does not overflow
            assert_ne!(destination_texture.width(), 0);
            assert_ne!(destination_texture.height(), 0);

            // Make sure an absurdly large texture isn't emitted
            assert!(destination_texture.width() <= max_texture_size);
            assert!(destination_texture.height() <= max_texture_size);

            let egui::ImageData::Color(image) = &image_delta.image;
            let source_texture_view =
                Texture::from_storage(image.size[0], image.size[1], image.pixels.as_slice());

            let texture_update_offset = Vector2::from(image_delta.pos.unwrap_or([0, 0]));
            let x_range =
                texture_update_offset.x..(texture_update_offset.x + source_texture_view.width());
            let y_range =
                texture_update_offset.y..(texture_update_offset.y + source_texture_view.height());

            match destination_texture {
                StoredTexture::Color(texture) => texture.view_mut(x_range, y_range).map_from(
                    source_texture_view,
                    CopyMode::Nearest,
                    premultiplied_color,
                ),
                StoredTexture::Alpha(texture) => {
                    let colored_texel_count = image
                        .pixels
                        .iter()
                        .filter(|color| !is_premultiplied_white(**color))
                        .count();

                    if colored_texel_count != 0 {
                        tracing::warn!(
                            "Font texture update has {colored_texel_count} colored texels, which \
                             alpha only font storage will draw as white"
                        );
                    }

                    texture.view_mut(x_range, y_range).map_from(
                        source_texture_view,
                        CopyMode::Nearest,
                        |color| color.a(),
                    );
                }
            }
        }
    }
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
    T: Intermediate,
>(
    context: &egui::Context,
    full_output: FullOutput,
    mut target_texture: Texture<impl StorageMut<Pixel = P>>,
    textures: &TextureMap<T>,
    batch_size: usize,
) {
    assert_ne!(target_texture.width(), 0);
    assert_ne!(target_texture.height(), 0);

    let pixels_per_point = full_output.pixels_per_point;
    let mut target_texture = target_texture.as_view_mut();

    let groups: Vec<_> =
        reduce_geometry::<P, T>(context, full_output.shapes, pixels_per_point).collect();

    let mut run_start = 0;

    for (index, group) in groups.iter().enumerate() {
        let [Primitive::Callback(callback)] = group.primitives.as_slice() else {
            continue;
        };

        raster_groups(
            &groups[run_start..index],
            textures,
            target_texture.as_view_mut(),
            batch_size,
        );
        run_start = index + 1;

        let min = group.rect.min.map(|component| component.floor().to_index());
        let max = Point2::new(
            group
                .rect
                .max
                .x
                .ceil()
                .to_index()
                .min(target_texture.width()),
            group
                .rect
                .max
                .y
                .ceil()
                .to_index()
                .min(target_texture.height()),
        );

        let rect = Rectangle::from_min_and_max(min, max);

        if !rect.is_valid() {
            continue;
        }

        callback.0.paint(
            CallbackInfo { pixels_per_point },
            target_texture.view_mut(rect.min.x..rect.max.x, rect.min.y..rect.max.y),
        );
    }

    raster_groups(&groups[run_start..], textures, target_texture, batch_size);

    #[multiversion::inherit_target]
    fn raster_groups<
        P: From<Srgba<u8>> + Into<Srgba<u8>> + Send + Sync + Copy + 'static,
        T: Intermediate,
    >(
        groups: &[Group<P, T>],
        textures: &TextureMap<T>,
        target: RefMutTexture<'_, P>,
        batch_size: usize,
    ) {
        if groups.is_empty() {
            return;
        }

        let band_count = rayon::current_num_threads().clamp(1, target.height());
        let bands = target.split_into_bands_mut(band_count);

        let mut cursor = 0;
        let ranges: Vec<_> = bands
            .iter()
            .map(|band| {
                let range = RangeInclusive::from_start_and_length(cursor, band.height());
                cursor += band.height();
                range
            })
            .collect();

        bands.into_par_iter().zip(ranges).for_each(
            #[inline]
            |(mut band, band_range)| {
                let offset = T::from_usize(*band_range.start()).unwrap();

                for group in groups {
                    let clip = Rectangle::from_min_and_max(
                        Point2::new(group.rect.min.x, group.rect.min.y - offset),
                        Point2::new(group.rect.max.x, group.rect.max.y - offset),
                    );

                    if !clip.is_valid() || !group.rect.is_valid() {
                        continue;
                    }

                    for primitive in group.primitives.iter() {
                        match primitive {
                            Primitive::SolidQuad(quad) => {
                                let mut quad = *quad;
                                quad.rectangle.min.y -= offset;
                                quad.rectangle.max.y -= offset;

                                fill_quad(clip, quad, band.as_view_mut());
                            }
                            Primitive::Triangle {
                                shape: triangle,
                                texture_id,
                            } => {
                                let mut triangle = *triangle;
                                triangle.v0.position.y -= offset;
                                triangle.v1.position.y -= offset;
                                triangle.v2.position.y -= offset;

                                match &textures[texture_id] {
                                    StoredTexture::Color(texture) => fill_triangle(
                                        clip,
                                        triangle,
                                        texture.as_view(),
                                        band.as_view_mut(),
                                        batch_size,
                                    ),
                                    StoredTexture::Alpha(texture) => fill_triangle(
                                        clip,
                                        triangle,
                                        texture.as_view(),
                                        band.as_view_mut(),
                                        batch_size,
                                    ),
                                }
                            }
                            Primitive::Callback { .. } => {
                                unreachable!()
                            }
                        }
                    }
                }
            },
        );
    }
}

#[inline]
pub(crate) fn premultiplied_color<T: Intermediate>(
    color: egui::Color32,
) -> PreAlpha<Srgb<Channel<T>>> {
    let color: Srgba<Channel<T>> = Srgba::from_components(color.to_tuple()).into_format();

    PreAlpha {
        color: color.color,
        alpha: color.alpha,
    }
}

#[inline]
fn is_premultiplied_white(color: Color32) -> bool {
    let [red, green, blue, alpha] = color.to_array();

    red == alpha && green == alpha && blue == alpha
}

#[derive(Debug)]
enum StoredTexture<T: Intermediate> {
    Color(OwnedTexture<PreAlpha<Srgb<Channel<T>>>>),
    Alpha(OwnedTexture<u8>),
}

impl<T: Intermediate> StoredTexture<T> {
    fn width(&self) -> usize {
        match self {
            Self::Color(texture) => texture.width(),
            Self::Alpha(texture) => texture.width(),
        }
    }

    fn height(&self) -> usize {
        match self {
            Self::Color(texture) => texture.height(),
            Self::Alpha(texture) => texture.height(),
        }
    }
}
