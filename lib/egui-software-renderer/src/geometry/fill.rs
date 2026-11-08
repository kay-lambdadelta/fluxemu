use std::ops::RangeInclusive;

use fluxemu_graphics::texture::{AsViewTexture, Storage, StorageMut, Texture};
use fluxemu_math::{range::ContiguousRange, rectangle::Rectangle};
use nalgebra::{Point2, SMatrix, Vector2, Vector3};
use palette::{
    Alpha, Srgb, Srgba,
    blend::{Compose, PreAlpha},
};

use crate::{
    geometry::{SolidQuad, Triangle},
    intermediate::{Channel, Intermediate},
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
    let texture_size = Point2::from(target_texture.size().map(|i| I::from_usize(i).unwrap()));

    let min = I::component_max(solid_quad.rectangle.min, clip.min)
        .map(|component| (component - I::half()).ceil().max(I::zero()));

    let max = I::component_min(
        I::component_min(solid_quad.rectangle.max, clip.max)
            .map(|component| (component - I::half()).ceil()),
        texture_size,
    );

    if min.x >= max.x || min.y >= max.y {
        return;
    }

    let rect = Rectangle::from_min_and_max(min.map(I::to_index), max.map(I::to_index));

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
            .map(|size| I::from_usize(size - 1).unwrap()),
    );

    let first_pixel = |component: I| (component - I::half()).ceil();
    let last_pixel = |component: I| (component - I::half()).ceil() - I::one();

    let vertex_max = I::component_max(
        I::component_max(triangle.v0.position, triangle.v1.position),
        triangle.v2.position,
    );

    // Clip the clipping box by the target texture size
    let clip_max = I::component_min(clip.max.map(last_pixel), max_texture_coordinates);

    // Clip the triangle
    let triangle_bounding_max = I::component_min(vertex_max.map(last_pixel), clip_max);

    let vertex_min = I::component_min(
        I::component_min(triangle.v0.position, triangle.v1.position),
        triangle.v2.position,
    );

    // Ensure negative clip values do not exist
    let clip_min = clip
        .min
        .map(|component| first_pixel(component).max(I::zero()));

    // Clip the triangle again
    let triangle_bounding_min = I::component_max(vertex_min.map(first_pixel), clip_min);

    // Guard against degenerate triangles
    if triangle_bounding_min.x > triangle_bounding_max.x
        || triangle_bounding_min.y > triangle_bounding_max.y
    {
        return;
    }

    let edges = [
        (triangle.v1.position, triangle.v2.position),
        (triangle.v2.position, triangle.v0.position),
        (triangle.v0.position, triangle.v1.position),
    ];

    // Units of which the pixel iteration machine will be advanced incrementally
    let step_x = Vector3::new(triangle.edge1.y, triangle.edge2.y, triangle.edge0.y)
        / triangle.signed_double_area;

    let step_uv = Vector2::new(
        Vector3::new(triangle.v0.uv.x, triangle.v1.uv.x, triangle.v2.uv.x)
            .component_mul(&step_x)
            .sum(),
        Vector3::new(triangle.v0.uv.y, triangle.v1.uv.y, triangle.v2.uv.y)
            .component_mul(&step_x)
            .sum(),
    );

    let step_color = PreAlpha {
        color: Srgb::new(
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
        ),
        alpha: Channel(step_x.dot(&Vector3::new(
            triangle.v0.color.alpha.0,
            triangle.v1.color.alpha.0,
            triangle.v2.color.alpha.0,
        ))),
    };

    let source_pixels = source_texture
        .as_slice()
        .expect("Source textures are always stored whole");
    assert!(source_pixels.len() <= u32::MAX as usize);
    let source_size = source_texture.size().map(|size| size as u32);

    for y in triangle_bounding_min.y.to_index()..=triangle_bounding_max.y.to_index() {
        let row_center = I::from_usize(y).unwrap() + I::half();

        let mut x_enter = triangle_bounding_min.x;
        let mut x_exit = triangle_bounding_max.x;

        for (start, end) in edges {
            if start.y == end.y {
                continue;
            }

            let (top, bottom) = if start.y < end.y {
                (start, end)
            } else {
                (end, start)
            };

            let crossing = top.x + (row_center - top.y) * (bottom.x - top.x) / (bottom.y - top.y);
            let boundary = first_pixel(crossing);

            let is_left_edge = (start.y > end.y) == (triangle.signed_double_area > I::zero());

            if is_left_edge {
                x_enter = x_enter.max(boundary);
            } else {
                x_exit = x_exit.min(boundary - I::one());
            }
        }

        if x_enter > x_exit {
            continue;
        }

        let barycentric_coordinates =
            barycentric_coordinates(Point2::new(x_enter + I::half(), row_center), &triangle);

        let x_enter = x_enter.to_index();
        let x_exit = x_exit.to_index();

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
                    source_pixels,
                    source_size,
                    current_uv,
                    current_color,
                    step_uv,
                    step_color,
                ),
                16 => pixel_rounds::<16, P, I>(
                    target_row,
                    source_pixels,
                    source_size,
                    current_uv,
                    current_color,
                    step_uv,
                    step_color,
                ),
                8 => pixel_rounds::<8, P, I>(
                    target_row,
                    source_pixels,
                    source_size,
                    current_uv,
                    current_color,
                    step_uv,
                    step_color,
                ),
                4 => pixel_rounds::<4, P, I>(
                    target_row,
                    source_pixels,
                    source_size,
                    current_uv,
                    current_color,
                    step_uv,
                    step_color,
                ),
                2 => pixel_rounds::<2, P, I>(
                    target_row,
                    source_pixels,
                    source_size,
                    current_uv,
                    current_color,
                    step_uv,
                    step_color,
                ),
                1 => pixel_rounds::<1, P, I>(
                    target_row,
                    source_pixels,
                    source_size,
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
            let len_number = I::from_usize(len).unwrap();
            current_uv += step_uv * len_number;
            current_color += step_color * Channel(len_number);
        }
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
    texture_pixels: &[PreAlpha<Srgb<Channel<I>>>],
    texture_size: Vector2<u32>,
    current_uv: Vector2<I>,
    current_color: PreAlpha<Srgb<Channel<I>>>,
    step_uv: Vector2<I>,
    step_color: PreAlpha<Srgb<Channel<I>>>,
) {
    // Assert these dimensions so the compiler doesn't forget
    //
    // This will almost certainly be optimized away in release mode
    assert_eq!(target_row.width(), C);
    assert_eq!(target_row.height(), 1);

    let mut interpolated_uvs: SMatrix<_, C, 2> = SMatrix::from_element(I::zero());
    for index in 0..C {
        let uv = current_uv + (step_uv * I::from_usize(index).unwrap());

        interpolated_uvs.row_mut(index).copy_from(&uv.transpose());
    }

    let mut interpolated_colors: SMatrix<_, C, 4> = SMatrix::from_element(Channel(I::zero()));
    for index in 0..C {
        let color = current_color + step_color * Channel(I::from_usize(index).unwrap());

        interpolated_colors
            .row_mut(index)
            .copy_from_slice(color.as_ref());
    }

    let texture_dimensions = texture_size.map(|size| I::from_u32(size).unwrap());

    let mut scaled_uvs: SMatrix<_, C, 2> = SMatrix::from_element(I::zero());
    for index in 0..C {
        let scaled_uv = texture_dimensions
            .transpose()
            .component_mul(&interpolated_uvs.row(index));

        scaled_uvs.row_mut(index).copy_from(&scaled_uv);
    }

    // We guard against any zero sized textures appearing already (in reduce.rs), so this can't underflow
    let max_possible_texture_coordinate = texture_size.map(|size| size - 1);

    let mut texture_coordinates: SMatrix<_, C, 2> = SMatrix::from_element(0);
    for index in 0..C {
        let texture_coordinate = scaled_uvs
            .row(index)
            .transpose()
            // Clamp to the edge
            .zip_map(&max_possible_texture_coordinate, I::to_texel_index);

        texture_coordinates
            .row_mut(index)
            .copy_from(&texture_coordinate.transpose());
    }

    let mut sampled_texture_color: SMatrix<_, C, 4> = SMatrix::from_element(Channel(I::zero()));
    for index in 0..C {
        let texture_coordinate = texture_coordinates.row(index);
        let texture_index = texture_coordinate[1] * texture_size.x + texture_coordinate[0];

        // SAFETY:
        //  We clamped to the texture size while producing the texture coordinates, and the caller
        //  ensures the texture has at most u32::MAX pixels so the index can't overflow
        let texture_pixel = *unsafe { texture_pixels.get_unchecked(texture_index as usize) };

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

        let inverse_alpha = if output_color.alpha.0.is_zero() {
            Channel(I::zero())
        } else {
            Channel(I::one()) / output_color.alpha
        };
        let output_color: Srgba<Channel<I>> = Alpha {
            color: output_color.color * inverse_alpha,
            alpha: output_color.alpha,
        };

        target_row[Point2::new(index, 0)] = output_color.into_format().into();
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
