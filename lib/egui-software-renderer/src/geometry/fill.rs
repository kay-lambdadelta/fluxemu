use std::ops::RangeInclusive;

use fluxemu_graphics::texture::{AsViewTexture, Storage, StorageMut, Texture};
use fluxemu_math::{range::ContiguousRange, rectangle::Rectangle};
use nalgebra::{Point2, SMatrix, Vector2, Vector3};
use palette::{
    Srgb, Srgba,
    blend::{Compose, PreAlpha},
};

use crate::{
    geometry::{SolidQuad, Triangle},
    intermediate::{
        self, Channel, Intermediate, ceil, floor, from_usize, half, to_index, to_texel_index,
    },
    powerof2::PowerOfTwoIter,
};

#[inline(always)]
pub fn fill_quad<
    P: From<Srgba<u8>> + Into<Srgba<u8>> + Send + Sync + Copy + 'static,
    I: Intermediate,
>(
    clip: Rectangle<I>,
    solid_quad: SolidQuad<I>,
    mut target_texture: Texture<impl StorageMut<Pixel = P>>,
) {
    let texture_max = Point2::from(target_texture.size().map(|size| from_usize::<I>(size - 1)));

    let min = intermediate::component_max(solid_quad.rectangle.min, clip.min)
        .map(|component| intermediate::max(component, I::zero()));

    let max = intermediate::component_min(
        intermediate::component_min(solid_quad.rectangle.max, clip.max),
        texture_max,
    );

    let rect = Rectangle::from_min_and_max(min, max);
    if !rect.is_valid() {
        return;
    }

    let rect = Rectangle::from_min_and_max(rect.min.map(to_index), rect.max.map(to_index));

    target_texture
        .view_mut(rect.x_range(), rect.y_range())
        .fill(solid_quad.color.into_format().into());
}

#[inline(always)]
pub fn fill_triangle<
    P: From<Srgba<u8>> + Into<Srgba<u8>> + Send + Sync + Copy + 'static,
    I: Intermediate,
    const BATCH_SIZE: usize,
>(
    clip: Rectangle<I>,
    triangle: Triangle<I>,
    source_texture: Texture<impl Storage<Pixel = PreAlpha<Srgb<Channel<I>>>>>,
    mut destination_texture: Texture<impl StorageMut<Pixel = P>>,
) {
    let source_texture = source_texture.as_view();

    let max_texture_coordinates = Point2::from(
        destination_texture
            .size()
            .map(|size| from_usize::<I>(size - 1)),
    );

    let vector_max = intermediate::component_max(
        intermediate::component_max(triangle.v0.position, triangle.v1.position),
        triangle.v2.position,
    );

    // Clip the clipping box by the target texture size
    let clip_max = intermediate::component_min(clip.max, max_texture_coordinates);

    // Clip the triangle
    let triangle_bounding_max = intermediate::component_min(vector_max, clip_max).map(floor);

    let vertex_min = intermediate::component_min(
        intermediate::component_min(triangle.v0.position, triangle.v1.position),
        triangle.v2.position,
    );

    // Ensure negative clip values do not exist
    let clip_min = clip
        .min
        .map(|component| intermediate::max(component, I::zero()));

    // Clip the triangle again
    let triangle_bounding_min = intermediate::component_max(vertex_min, clip_min).map(ceil);

    // Guard against degenerate triangles
    if triangle_bounding_min.x > triangle_bounding_max.x
        || triangle_bounding_min.y > triangle_bounding_max.y
    {
        return;
    }

    let mut barycentric_coordinates = barycentric_coordinates(
        // Offset to the center of the pixel
        triangle_bounding_min + Vector2::from_element(half::<I>()),
        &triangle,
    );
    let mut row_start_barycentric_coordinates = barycentric_coordinates;

    // Units of which the pixel iteration machine will be advanced incrementally
    let step_x = Vector3::new(triangle.edge1.y, triangle.edge2.y, triangle.edge0.y)
        / triangle.signed_double_area;

    let step_y = Vector3::new(
        triangle.v2.position.x - triangle.v1.position.x,
        triangle.v0.position.x - triangle.v2.position.x,
        triangle.v1.position.x - triangle.v0.position.x,
    ) / triangle.signed_double_area;

    let step_uv = Vector2::new(
        Vector3::new(triangle.v0.uv.x, triangle.v1.uv.x, triangle.v2.uv.x)
            .component_mul(&step_x)
            .sum(),
        Vector3::new(triangle.v0.uv.y, triangle.v1.uv.y, triangle.v2.uv.y)
            .component_mul(&step_x)
            .sum(),
    );

    let step_color = Srgba::new(
        Channel(step_x.dot(&Vector3::new(
            triangle.v0.color.red.0,
            triangle.v1.color.red.0,
            triangle.v2.color.red.0,
        ))),
        Channel(step_x.dot(&Vector3::new(
            triangle.v0.color.green.0,
            triangle.v1.color.green.0,
            triangle.v2.color.green.0,
        ))),
        Channel(step_x.dot(&Vector3::new(
            triangle.v0.color.blue.0,
            triangle.v1.color.blue.0,
            triangle.v2.color.blue.0,
        ))),
        Channel(step_x.dot(&Vector3::new(
            triangle.v0.color.alpha.0,
            triangle.v1.color.alpha.0,
            triangle.v2.color.alpha.0,
        ))),
    );

    let texture_dimensions = source_texture.size().map(from_usize::<I>);

    for y in to_index(triangle_bounding_min.y)..=to_index(triangle_bounding_max.y) {
        // This calculates the enter and exit point of which this particular scanline will be relevant
        // to the triangle we are drawing
        let x_enter = (0..3)
            .map(|index| {
                (if step_x[index] > I::zero() {
                    triangle_bounding_min.x
                        - row_start_barycentric_coordinates[index] / step_x[index]
                } else {
                    triangle_bounding_min.x
                }) - half::<I>()
            })
            .fold(triangle_bounding_min.x, intermediate::max);
        let x_enter = to_index(ceil(x_enter));

        let x_exit = (0..3)
            .map(|index| {
                (if step_x[index] < I::zero() {
                    triangle_bounding_min.x
                        - row_start_barycentric_coordinates[index] / step_x[index]
                } else {
                    triangle_bounding_max.x
                }) + half::<I>()
            })
            .fold(triangle_bounding_max.x, intermediate::min);
        let x_exit = to_index(floor(x_exit));

        // Advance coordinates
        barycentric_coordinates = row_start_barycentric_coordinates
            + step_x * (from_usize::<I>(x_enter) - triangle_bounding_min.x);

        let mut current_uv = triangle.v0.uv.coords * barycentric_coordinates.x
            + triangle.v1.uv.coords * barycentric_coordinates.y
            + triangle.v2.uv.coords * barycentric_coordinates.z;

        let mut current_color = triangle.v0.color * Channel(barycentric_coordinates.x)
            + triangle.v1.color * Channel(barycentric_coordinates.y)
            + triangle.v2.color * Channel(barycentric_coordinates.z);

        let x_range = x_enter..=x_exit;
        let mut x = *x_range.start();

        // This power of two iterator forcing constant run lengths makes very efficient simd code
        for len in PowerOfTwoIter::<BATCH_SIZE>::new(x_range.len()) {
            let target_row =
                destination_texture.view_mut(RangeInclusive::from_start_and_length(x, len), y..=y);

            match len {
                32 => pixel_rounds::<32, P, I>(
                    target_row,
                    source_texture,
                    texture_dimensions,
                    current_uv,
                    current_color,
                    step_uv,
                    step_color,
                ),
                16 => pixel_rounds::<16, P, I>(
                    target_row,
                    source_texture,
                    texture_dimensions,
                    current_uv,
                    current_color,
                    step_uv,
                    step_color,
                ),
                8 => pixel_rounds::<8, P, I>(
                    target_row,
                    source_texture,
                    texture_dimensions,
                    current_uv,
                    current_color,
                    step_uv,
                    step_color,
                ),
                4 => pixel_rounds::<4, P, I>(
                    target_row,
                    source_texture,
                    texture_dimensions,
                    current_uv,
                    current_color,
                    step_uv,
                    step_color,
                ),
                2 => pixel_rounds::<2, P, I>(
                    target_row,
                    source_texture,
                    texture_dimensions,
                    current_uv,
                    current_color,
                    step_uv,
                    step_color,
                ),
                1 => pixel_rounds::<1, P, I>(
                    target_row,
                    source_texture,
                    texture_dimensions,
                    current_uv,
                    current_color,
                    step_uv,
                    step_color,
                ),
                _ => {
                    unreachable!("Guard against absurdly large batch size failed");
                }
            }

            // Advance everything
            x += len;
            let len_number = from_usize::<I>(len);
            barycentric_coordinates += step_x * len_number;
            current_uv += step_uv * len_number;
            current_color += step_color * Channel(len_number);
        }

        row_start_barycentric_coordinates += step_y;
    }
}

// Note that most of these huge stack allocations will be eliminated in release mode
//
// They exist so that each operation can be simplified in its own loop, and fused together as LLVM pleases

#[inline(always)]
fn pixel_rounds<
    const C: usize,
    P: From<Srgba<u8>> + Into<Srgba<u8>> + Send + Sync + Copy + 'static,
    I: Intermediate,
>(
    mut target_row: Texture<impl StorageMut<Pixel = P>>,
    texture: Texture<impl Storage<Pixel = PreAlpha<Srgb<Channel<I>>>>>,
    texture_dimensions: Vector2<I>,
    current_uv: Vector2<I>,
    current_color: Srgba<Channel<I>>,
    step_uv: Vector2<I>,
    step_color: Srgba<Channel<I>>,
) {
    // Assert these dimensions so the compiler doesn't forget
    //
    // This will almost certainly be optimized away in release mode
    assert_eq!(target_row.width(), C);
    assert_eq!(target_row.height(), 1);

    let mut interpolated_uvs: SMatrix<_, C, 2> = SMatrix::from_element(I::zero());
    for index in 0..C {
        let uv = current_uv + (step_uv * from_usize::<I>(index));

        interpolated_uvs.row_mut(index).copy_from(&uv.transpose());
    }

    let mut interpolated_colors: SMatrix<_, C, 4> = SMatrix::from_element(Channel(I::zero()));
    for index in 0..C {
        let color = current_color + (step_color * Channel(from_usize::<I>(index)));

        interpolated_colors
            .row_mut(index)
            .copy_from_slice(color.premultiply().as_ref());
    }

    let mut scaled_uvs: SMatrix<_, C, 2> = SMatrix::from_element(I::zero());
    for index in 0..C {
        let scaled_uv = texture_dimensions
            .transpose()
            .component_mul(&interpolated_uvs.row(index));

        scaled_uvs.row_mut(index).copy_from(&scaled_uv);
    }

    // A coordinate being emitted somehow that ended up being beyond u32::MAX would be absurd, but clamp just in case
    //
    // We guard against any zero sized textures appearing already (in reduce.rs), so this can't underflow
    let max_possible_texture_coordinate = texture
        .size()
        .map(|size| (size - 1).min(u32::MAX as usize) as u32);

    let mut texture_coordinates: SMatrix<_, C, 2> = SMatrix::from_element(0);
    for index in 0..C {
        let texture_coordinate = scaled_uvs
            .row(index)
            .transpose()
            // Clamp to the edge
            .zip_map(&max_possible_texture_coordinate, to_texel_index);

        texture_coordinates
            .row_mut(index)
            .copy_from(&texture_coordinate.transpose());
    }

    let mut sampled_texture_color: SMatrix<_, C, 4> = SMatrix::from_element(Channel(I::zero()));
    for index in 0..C {
        let texture_coordinate = texture_coordinates.row(index).transpose().cast();

        // SAFETY:
        //  We clamped to the texture size while producing the texture coordinates
        let texture_pixel = *unsafe { texture.get_unchecked(texture_coordinate) };

        sampled_texture_color
            .row_mut(index)
            .copy_from_slice(texture_pixel.as_ref());
    }

    let mut source_colors: SMatrix<_, C, 4> = SMatrix::from_element(Channel(I::zero()));
    for index in 0..C {
        let interpolated_color = interpolated_colors.row(index);
        let sampled_texture_color = sampled_texture_color.row(index);

        let source_color = sampled_texture_color.component_mul(&interpolated_color);

        source_colors.row_mut(index).copy_from(&source_color);
    }

    let mut destination_colors: SMatrix<_, C, 4> = SMatrix::from_element(Channel(I::zero()));
    for index in 0..C {
        let destination_color: Srgba<Channel<I>> =
            target_row[Point2::new(index, 0)].into().into_format();

        destination_colors
            .row_mut(index)
            .copy_from_slice(destination_color.premultiply().as_ref());
    }

    let mut output_colors: SMatrix<_, C, 4> = SMatrix::from_element(Channel(I::zero()));
    for index in 0..C {
        let source_color = source_colors.row(index).transpose();
        let source_color: PreAlpha<Srgb<Channel<I>>> = PreAlpha::from(<[_; 4]>::from(source_color));

        let destination_color = destination_colors.row(index).transpose();
        let destination_color: PreAlpha<Srgb<Channel<I>>> =
            PreAlpha::from(<[_; 4]>::from(destination_color));

        let output_color = source_color.over(destination_color);

        output_colors
            .row_mut(index)
            .copy_from_slice(output_color.as_ref());
    }

    for index in 0..C {
        let output_color = output_colors.row(index).transpose();
        let output_color: PreAlpha<Srgb<Channel<I>>> = PreAlpha::from(<[_; 4]>::from(output_color));

        target_row[Point2::new(index, 0)] = output_color.unpremultiply().into_format().into();
    }
}

#[inline]
fn barycentric_coordinates<T: Intermediate>(
    point: Point2<T>,
    triangle: &Triangle<T>,
) -> Vector3<T> {
    let v0p = triangle.v0.position - point;
    let v1p = triangle.v1.position - point;
    let v2p = triangle.v2.position - point;

    let area = Vector3::new(v1p.perp(&v2p), v2p.perp(&v0p), v0p.perp(&v1p));

    area / triangle.signed_double_area
}
