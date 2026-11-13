use fluxemu_graphics::texture::{CopyMode, OwnedTexture, RefMutTexture};
use image::{DynamicImage, RgbaImage};
use nalgebra::Vector2;
use palette::Srgba;
use ratatui::{
    Frame,
    layout::{Rect, Size},
};
use ratatui_image::{FontSize, Image, Resize, picker::Picker, protocol::Protocol};

#[derive(Default)]
pub struct MachineView {
    encoded_frame: Option<EncodedFrame>,
}

struct EncodedFrame {
    area: Rect,
    protocol: Protocol,
}

impl MachineView {
    pub fn clear(&mut self) {
        self.encoded_frame = None;
    }

    pub fn draw(
        &mut self,
        frame: &mut Frame,
        area: Rect,
        picker: &Picker,
        refresh: bool,
        capture_framebuffer: impl FnOnce() -> Option<(OwnedTexture<Srgba<u8>>, Vector2<f32>)>,
    ) {
        let is_stale = self
            .encoded_frame
            .as_ref()
            .is_none_or(|encoded_frame| encoded_frame.area != area);

        if (refresh || is_stale)
            && let Some((framebuffer, display_size)) = capture_framebuffer()
        {
            let image = scale_to_cells(
                &framebuffer,
                display_size,
                area.as_size(),
                picker.font_size(),
            );

            match picker.new_protocol(image, area.as_size(), Resize::Fit(None)) {
                Ok(protocol) => {
                    self.encoded_frame = Some(EncodedFrame { area, protocol });
                }
                Err(err) => {
                    tracing::error!("Failed to encode framebuffer for the terminal: {}", err);
                }
            }
        }

        if let Some(encoded_frame) = &self.encoded_frame {
            let image_area = encoded_frame.protocol.size();
            let centered_area = Rect {
                x: area.x + area.width.saturating_sub(image_area.width) / 2,
                y: area.y + area.height.saturating_sub(image_area.height) / 2,
                width: image_area.width.min(area.width),
                height: image_area.height.min(area.height),
            };

            frame.render_widget(Image::new(&encoded_frame.protocol), centered_area);
        }
    }
}

fn scale_to_cells(
    framebuffer: &OwnedTexture<Srgba<u8>>,
    display_size: Vector2<f32>,
    area: Size,
    font_size: FontSize,
) -> DynamicImage {
    let font_size = Vector2::new(f64::from(font_size.width), f64::from(font_size.height));
    let source_size = display_size.cast::<f64>();
    let available_size =
        Vector2::new(f64::from(area.width), f64::from(area.height)).component_mul(&font_size);

    let scale = available_size.component_div(&source_size).min();
    let cells = (source_size * scale)
        .map(f64::round)
        .component_div(&font_size)
        .map(|cells| cells.ceil().max(1.0));
    let size = cells.component_mul(&font_size).map(|side| side as usize);

    let mut image = RgbaImage::new(size.x as u32, size.y as u32);
    let (pixels, _) = image.as_chunks_mut::<4>();

    RefMutTexture::from_storage(size.x, size.y, pixels).map_from(
        framebuffer,
        CopyMode::Nearest,
        |pixel: Srgba<u8>| [pixel.red, pixel.green, pixel.blue, u8::MAX],
    );

    DynamicImage::ImageRgba8(image)
}
