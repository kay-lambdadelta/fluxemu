use std::ops::RangeInclusive;

use fluxemu_graphics::api::software::texture::{
    Texture, TextureImpl, TextureImplMut, TextureViewMut,
};
use fluxemu_range::ContiguousRange;
use nalgebra::{Point2, SMatrix, Vector2, Vector3};
use palette::{
    Srgb, Srgba,
    blend::{Compose, PreAlpha},
};

use crate::{
    geometry::{Shape, SolidQuad, Triangle},
    powerof2::PowerOfTwoIter,
};

#[inline(always)]
pub fn fill_quad<P: From<Srgba<u8>> + Into<Srgba<u8>> + Send + Sync + Copy + 'static>(
    shape: &Shape,
    solid_quad: SolidQuad,
    target_texture: &mut TextureViewMut<'_, P>,
) {
    let texture_max = Point2::from(target_texture.size() - Vector2::from_element(1));

    let min = solid_quad
        .min
        .sup(&shape.min)
        .map(|c| c as usize)
        .inf(&texture_max);

    let max = solid_quad
        .max
        .inf(&shape.max)
        .map(|c| c as usize)
        .inf(&texture_max);

    let mut region = target_texture.view_mut(min.x..max.x, min.y..max.y);
    region.fill(solid_quad.color.into_format().into());
}

#[inline(always)]
pub fn fill_triangle<
    P: From<Srgba<u8>> + Into<Srgba<u8>> + Send + Sync + Copy + 'static,
    const BATCH_SIZE: usize,
>(
    geometry: &Shape,
    triangle: Triangle,
    source_texture: &Texture<PreAlpha<Srgb<f32>>>,
    destination_texture: &mut TextureViewMut<'_, P>,
) {
    let target_texture_dimensions = destination_texture.size().cast();
    let max_texture_coordinates =
        Point2::from(target_texture_dimensions - Vector2::from_element(1.0));

    let vector_max = Point2::new(
        Vector3::new(
            triangle.v0.position.x,
            triangle.v1.position.x,
            triangle.v2.position.x,
        )
        .max(),
        Vector3::new(
            triangle.v0.position.y,
            triangle.v1.position.y,
            triangle.v2.position.y,
        )
        .max(),
    );

    // Clip the clipping box by the target texture size
    let clip_max = geometry.max.inf(&max_texture_coordinates);

    // Clip the triangle
    let triangle_bounding_max = vector_max.inf(&clip_max).map(|c| c.floor());

    let vertex_min = Point2::new(
        Vector3::new(
            triangle.v0.position.x,
            triangle.v1.position.x,
            triangle.v2.position.x,
        )
        .min(),
        Vector3::new(
            triangle.v0.position.y,
            triangle.v1.position.y,
            triangle.v2.position.y,
        )
        .min(),
    );

    // Ensure negative clip values do not exist
    let clip_min = geometry.min.sup(&Point2::new(0.0, 0.0));

    // Clip the triangle again
    let triangle_bounding_min = vertex_min.sup(&clip_min).map(|c| c.ceil());

    // Guard against degenerate triangles
    if triangle_bounding_min.x > triangle_bounding_max.x
        || triangle_bounding_min.y > triangle_bounding_max.y
    {
        return;
    }

    let mut barycentric_coordinates = barycentric_coordinates(
        // Offset to the center of the pixel
        triangle_bounding_min + Vector2::from_element(0.5),
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

    let texture_dimensions: Vector2<f32> = source_texture.size().cast();

    for y in triangle_bounding_min.y as usize..=triangle_bounding_max.y as usize {
        // This calculates the enter and exit point of which this particular scanline will be relevant
        // to the triangle we are drawing

        let x_enter = (0..3)
            .map(|index| {
                (if step_x[index] > 0.0 {
                    triangle_bounding_min.x
                        - row_start_barycentric_coordinates[index] / step_x[index]
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
                        - row_start_barycentric_coordinates[index] / step_x[index]
                } else {
                    triangle_bounding_max.x
                }) + 0.5
            })
            .fold(triangle_bounding_max.x, f32::min)
            .floor() as usize;

        // Advance coordinates
        barycentric_coordinates =
            row_start_barycentric_coordinates + step_x * (x_enter as f32 - triangle_bounding_min.x);

        let mut current_uv = triangle.v0.uv.coords * barycentric_coordinates.x
            + triangle.v1.uv.coords * barycentric_coordinates.y
            + triangle.v2.uv.coords * barycentric_coordinates.z;

        let mut current_color = triangle.v0.color * barycentric_coordinates.x
            + triangle.v1.color * barycentric_coordinates.y
            + triangle.v2.color * barycentric_coordinates.z;

        let x_range = x_enter..=x_exit;
        let mut x = *x_range.start();

        // This power of two iterator forcing constant run lengths makes very efficient simd code
        for len in PowerOfTwoIter::<BATCH_SIZE>::new(x_range.len()) {
            let target_pixel_row =
                destination_texture.view_mut(RangeInclusive::from_start_and_length(x, len), y..=y);

            match len {
                32 => pixel_rounds::<32, P>(
                    target_pixel_row,
                    source_texture,
                    texture_dimensions,
                    current_uv,
                    current_color,
                    step_uv,
                    step_color,
                ),
                16 => pixel_rounds::<16, P>(
                    target_pixel_row,
                    source_texture,
                    texture_dimensions,
                    current_uv,
                    current_color,
                    step_uv,
                    step_color,
                ),
                8 => pixel_rounds::<8, P>(
                    target_pixel_row,
                    source_texture,
                    texture_dimensions,
                    current_uv,
                    current_color,
                    step_uv,
                    step_color,
                ),
                4 => pixel_rounds::<4, P>(
                    target_pixel_row,
                    source_texture,
                    texture_dimensions,
                    current_uv,
                    current_color,
                    step_uv,
                    step_color,
                ),
                2 => pixel_rounds::<2, P>(
                    target_pixel_row,
                    source_texture,
                    texture_dimensions,
                    current_uv,
                    current_color,
                    step_uv,
                    step_color,
                ),
                1 => pixel_rounds::<1, P>(
                    target_pixel_row,
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
            barycentric_coordinates += step_x * len as f32;
            current_uv += step_uv * len as f32;
            current_color += step_color * len as f32;
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
>(
    mut target_row: TextureViewMut<P>,
    texture: &Texture<PreAlpha<Srgb<f32>>>,
    texture_dimensions: Vector2<f32>,
    current_uv: Vector2<f32>,
    current_color: Srgba<f32>,
    step_uv: Vector2<f32>,
    step_color: Srgba<f32>,
) {
    // Assert these dimensions so the compiler doesn't forget
    //
    // This will almost certainly be optimized away in release mode
    assert_eq!(target_row.width(), C);
    assert_eq!(target_row.height(), 1);

    let mut interpolated_uvs: SMatrix<_, C, 2> = SMatrix::from_element(0.0);
    for index in 0..C {
        let uv = current_uv + (step_uv * index as f32);

        interpolated_uvs.row_mut(index).copy_from(&uv.transpose());
    }

    let mut interpolated_colors: SMatrix<_, C, 4> = SMatrix::from_element(0.0);
    for index in 0..C {
        let color = current_color + (step_color * index as f32);

        interpolated_colors
            .row_mut(index)
            .copy_from_slice(color.premultiply().as_ref());
    }

    let mut scaled_uvs: SMatrix<_, C, 2> = SMatrix::from_element(0.0);
    for index in 0..C {
        let scaled_uv = texture_dimensions
            .transpose()
            .component_mul(&interpolated_uvs.row(index));

        scaled_uvs.row_mut(index).copy_from(&scaled_uv);
    }

    // A coordinate being emitted somehow that ended up being beyond u32::MAX would be absurd, but clamp just in case
    //
    // We guard against any zero sized textures appearing already (in reduce.rs), so this can't underflow
    let max_possible_texture_coordinate = (texture.size().cast() - Vector2::from_element(1))
        .zip_map(&Vector2::from_element(u32::MAX as usize), |a, b| a.min(b));

    let mut texture_coordinates: SMatrix<_, C, 2> = SMatrix::from_element(0);
    for index in 0..C {
        let texture_coordinate = scaled_uvs
            .row(index)
            .transpose()
            .map(|c| {
                // SAFETY:
                //  We guard against any NaN values by clamping to the range [0, u32::MAX]
                //  This has to be done before the `to_int_unchecked` call to avoid Rust UB
                //
                // HACK:
                //  This is technically a hack to avoid some bad llvm.fptoui.sat codegen on x86/x86-64.
                //  It usually ends up scalarizing without using `to_int_unchecked`
                //  Revisit it once our MSRV is raised (written as of 1.95.0)
                unsafe { c.max(0.0).min(u32::MAX as f32).to_int_unchecked::<u32>() }
            })
            // Clamp to the edge
            .zip_map(&max_possible_texture_coordinate, |a, b| a.min(b as u32));

        texture_coordinates
            .row_mut(index)
            .copy_from(&texture_coordinate.transpose());
    }

    let mut sampled_texture_color: SMatrix<_, C, 4> = SMatrix::from_element(0.0);
    for index in 0..C {
        let texture_coordinate = texture_coordinates.row(index).transpose().cast();

        // SAFETY:
        //  We clamped to the texture size while producing the texture coordinates
        let texture_pixel = *unsafe { texture.get_unchecked(texture_coordinate) };

        sampled_texture_color
            .row_mut(index)
            .copy_from_slice(texture_pixel.as_ref());
    }

    let mut source_colors: SMatrix<_, C, 4> = SMatrix::from_element(0.0);
    for index in 0..C {
        let interpolated_color = interpolated_colors.row(index);
        let sampled_texture_color = sampled_texture_color.row(index);

        let source_color = sampled_texture_color.component_mul(&interpolated_color);

        source_colors.row_mut(index).copy_from(&source_color);
    }

    let mut destination_colors: SMatrix<_, C, 4> = SMatrix::from_element(0.0);
    for index in 0..C {
        let destination_color = target_row[Point2::new(index, 0)].into().into_format();

        destination_colors
            .row_mut(index)
            .copy_from_slice(destination_color.premultiply().as_ref());
    }

    let mut output_colors: SMatrix<_, C, 4> = SMatrix::from_element(0.0);
    for index in 0..C {
        let source_color = source_colors.row(index).transpose();
        let source_color: PreAlpha<Srgb<f32>> = PreAlpha::from(<[_; 4]>::from(source_color));

        let destination_color = destination_colors.row(index).transpose();
        let destination_color: PreAlpha<Srgb<f32>> =
            PreAlpha::from(<[_; 4]>::from(destination_color));

        let output_color = source_color.over(destination_color);

        output_colors
            .row_mut(index)
            .copy_from_slice(output_color.as_ref());
    }

    for index in 0..C {
        let output_color = output_colors.row(index).transpose();
        let output_color: PreAlpha<Srgb<f32>> = PreAlpha::from(<[_; 4]>::from(output_color));

        target_row[Point2::new(index, 0)] = output_color.unpremultiply().into_format().into();
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
