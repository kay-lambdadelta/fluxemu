use std::sync::Arc;

use fluxemu_graphics::{api::software::Software, texture::OwnedTexture};
use fluxemu_runtime::machine::Machine;
use image::{DynamicImage, RgbImage};
use palette::Srgba;
use ratatui::{Frame, layout::Rect};
use ratatui_image::{FilterType, Image, Resize, picker::Picker, protocol::Protocol};

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
        machine: &Arc<Machine>,
        refresh: bool,
    ) {
        let is_stale = self
            .encoded_frame
            .as_ref()
            .is_none_or(|encoded_frame| encoded_frame.area != area);

        if (refresh || is_stale)
            && let Some(image) = capture_framebuffer(machine)
        {
            match picker.new_protocol(
                image,
                area.as_size(),
                Resize::Scale(Some(FilterType::Nearest)),
            ) {
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

fn capture_framebuffer(machine: &Arc<Machine>) -> Option<DynamicImage> {
    let runtime = machine.enter_runtime();
    let framebuffer_path = runtime.list_framebuffers().next()?;

    runtime.read_framebuffer::<Software, _>(
        framebuffer_path,
        runtime.safe_advance_timestamp(),
        framebuffer2image,
    )
}

fn framebuffer2image(framebuffer: &OwnedTexture<Srgba<u8>>) -> DynamicImage {
    let mut image = RgbImage::new(framebuffer.width() as u32, framebuffer.height() as u32);

    for (destination, source) in image.pixels_mut().zip(framebuffer.iter_pixels()) {
        destination.0 = [source.red, source.green, source.blue];
    }

    DynamicImage::ImageRgb8(image)
}
