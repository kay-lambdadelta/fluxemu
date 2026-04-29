use nalgebra::{Point2, SMatrix, SVector, Vector2};
use palette::{Srgba, blend::Compose};

use crate::{
    egui_software_renderer::shapes::Triangle,
    texture::{TextureImpl, TextureViewMut},
};

#[inline(always)]
pub fn pixel_rounds<const C: usize, P: From<Srgba<u8>> + Into<Srgba<u8>> + Copy>(
    mut target_pixel_row: TextureViewMut<P>,
    triangle: &Triangle<'_>,
    texture_dimensions: Vector2<f32>,
    current_uv: &mut Vector2<f32>,
    current_color: &mut Srgba<f32>,
    step_uv: Vector2<f32>,
    step_color: Srgba<f32>,
) {
    // Const assert these dimensions so the compiler doesn't forget
    assert_eq!(target_pixel_row.width(), C);
    assert_eq!(target_pixel_row.height(), 1);

    // Calculate UVs
    let mut interpolated_uvs = SMatrix::<f32, 2, C>::from_element(0.0);
    for index in 0..C {
        let uv = *current_uv + (step_uv * index as f32);
        interpolated_uvs.column_mut(index).copy_from(&uv);
    }
    *current_uv += step_uv * C as f32;

    // Calculate colors
    let mut interpolated_colors = SVector::<Srgba<f32>, C>::from_element(Default::default());
    for index in 0..C {
        let color = *current_color + (step_color * index as f32);
        interpolated_colors[index] = color;
    }
    *current_color += step_color * C as f32;

    // Gather fetch
    let mut texture_pixels_red = SVector::<_, C>::from_element(0.0);
    let mut texture_pixels_green = SVector::<_, C>::from_element(0.0);
    let mut texture_pixels_blue = SVector::<_, C>::from_element(0.0);
    let mut texture_pixels_alpha = SVector::<_, C>::from_element(0.0);
    for index in 0..C {
        let uv = interpolated_uvs.column(index);

        let pixel_coords: Point2<_> = Point2::new(
            (texture_dimensions.x * uv.x) as usize,
            (texture_dimensions.y * uv.y) as usize,
        )
        .coords
        .zip_map(
            &(triangle.texture.size() - Vector2::from_element(1)),
            |a, b| a.min(b),
        )
        .into();

        let texture_pixel = unsafe { triangle.texture.get_unchecked(pixel_coords) };

        texture_pixels_red[index] = texture_pixel.red;
        texture_pixels_green[index] = texture_pixel.green;
        texture_pixels_blue[index] = texture_pixel.blue;
        texture_pixels_alpha[index] = texture_pixel.alpha;
    }

    // Read source pixels and tint by texture pixels
    let mut source_pixels_red = SVector::<_, C>::from_element(0.0);
    let mut source_pixels_green = SVector::<_, C>::from_element(0.0);
    let mut source_pixels_blue = SVector::<_, C>::from_element(0.0);
    let mut source_pixels_alpha = SVector::<_, C>::from_element(0.0);
    for index in 0..C {
        let color = interpolated_colors[index];

        source_pixels_red[index] = color.red * texture_pixels_red[index];
        source_pixels_green[index] = color.green * texture_pixels_green[index];
        source_pixels_blue[index] = color.blue * texture_pixels_blue[index];
        source_pixels_alpha[index] = color.alpha * texture_pixels_alpha[index];
    }

    // Read destination pixels
    let mut destination_pixels_red = SVector::<_, C>::from_element(0.0);
    let mut destination_pixels_green = SVector::<_, C>::from_element(0.0);
    let mut destination_pixels_blue = SVector::<_, C>::from_element(0.0);
    let mut destination_pixels_alpha = SVector::<_, C>::from_element(0.0);
    for index in 0..C {
        let pixel = target_pixel_row[Point2::new(index, 0)].into().into_format();

        destination_pixels_red[index] = pixel.red;
        destination_pixels_green[index] = pixel.green;
        destination_pixels_blue[index] = pixel.blue;
        destination_pixels_alpha[index] = pixel.alpha;
    }

    // Over composite
    for index in 0..C {
        let source = Srgba::new(
            source_pixels_red[index],
            source_pixels_green[index],
            source_pixels_blue[index],
            source_pixels_alpha[index],
        );

        let destination = Srgba::new(
            destination_pixels_red[index],
            destination_pixels_green[index],
            destination_pixels_blue[index],
            destination_pixels_alpha[index],
        );

        let output = source.over(destination);

        destination_pixels_red[index] = output.red;
        destination_pixels_green[index] = output.green;
        destination_pixels_blue[index] = output.blue;
        destination_pixels_alpha[index] = output.alpha;
    }

    // Write and pack back
    for index in 0..C {
        target_pixel_row[Point2::new(index, 0)] = Srgba::new(
            destination_pixels_red[index],
            destination_pixels_green[index],
            destination_pixels_blue[index],
            destination_pixels_alpha[index],
        )
        .into_format()
        .into();
    }
}
