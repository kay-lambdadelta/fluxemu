use std::{collections::HashMap, hint::black_box, sync::Arc};

use criterion::{BenchmarkId, Criterion, criterion_group, criterion_main};
use egui::{
    CentralPanel, Color32, ColorImage, Context, FontData, FontDefinitions, FontFamily, FontId,
    Image, Panel, ProgressBar, RawInput, Rect, ScrollArea, Slider, Stroke, TextStyle,
    TextureHandle, TextureOptions, Ui, ViewportId, ViewportInfo, Window,
};
use fixed::types::{I16F16, I32F32};
use fluxemu_egui_software_renderer::{Intermediate, Renderer};
use fluxemu_graphics::texture::{AsViewTextureMut, OwnedTexture};
use palette::{
    Srgba, WithAlpha,
    cast::Packed,
    named::BLACK,
    rgb::channels::{Bgra, Rgba},
};

const UNSCII_FONT: &[u8] = include_bytes!("../../../external/unscii/fontfiles/unscii-16.otf");

fn setup_context() -> Context {
    let context = Context::default();

    context.set_fonts(FontDefinitions {
        font_data: [(
            "unscii".to_string(),
            Arc::new(FontData::from_static(UNSCII_FONT)),
        )]
        .into_iter()
        .collect(),
        families: [
            (FontFamily::Proportional, vec!["unscii".to_string()]),
            (FontFamily::Monospace, vec!["unscii".to_string()]),
        ]
        .into_iter()
        .collect(),
    });

    context.global_style_mut(|style| {
        style
            .text_styles
            .insert(TextStyle::Body, FontId::new(18.0, FontFamily::Proportional));
        style.text_styles.insert(
            TextStyle::Button,
            FontId::new(20.0, FontFamily::Proportional),
        );
        style.text_styles.insert(
            TextStyle::Heading,
            FontId::new(24.0, FontFamily::Proportional),
        );
    });

    context
}

#[derive(Default)]
struct Scene {
    texture: Option<TextureHandle>,
    volume: f32,
    enabled: bool,
}

impl Scene {
    fn ui(&mut self, ui: &mut Ui) {
        let texture = self.texture.get_or_insert_with(|| {
            let size = 64;
            let pixels = (0..size * size)
                .map(|index| {
                    let x = (index % size) as u8;
                    let y = (index / size) as u8;
                    Color32::from_rgb(x * 4, y * 4, (x ^ y) * 4)
                })
                .collect();

            ui.ctx().load_texture(
                "scene",
                ColorImage::new([size, size], pixels),
                TextureOptions::LINEAR,
            )
        });

        Panel::top("menu").show(ui, |ui| {
            ui.horizontal(|ui| {
                for name in ["File", "Machine", "Settings", "Help"] {
                    let _ = ui.button(name);
                }
            });
        });

        Panel::left("library").show(ui, |ui| {
            ui.heading("Library");
            ScrollArea::vertical().show(ui, |ui| {
                for index in 0..64 {
                    ui.label(format!("Program {index:02}"));
                }
            });
        });

        CentralPanel::default().show(ui, |ui| {
            let rect = ui.max_rect();
            let painter = ui.painter_at(rect);

            for index in 0..32 {
                let offset = index as f32 * 24.0;
                painter.rect_filled(
                    Rect::from_min_size(
                        rect.min + [offset, offset * 0.5].into(),
                        [160.0, 96.0].into(),
                    ),
                    8.0,
                    Color32::from_rgba_unmultiplied(200, 80, 40, 96),
                );
                painter.circle_filled(
                    rect.center() + [offset - 384.0, 0.0].into(),
                    48.0,
                    Color32::from_rgba_unmultiplied(40, 120, 220, 96),
                );
                painter.line_segment(
                    [rect.left_bottom(), rect.min + [offset * 4.0, 0.0].into()],
                    Stroke::new(2.0, Color32::WHITE),
                );
            }

            ui.add(Image::new((texture.id(), [256.0, 256.0].into())));
        });

        let context = ui.ctx().clone();

        Window::new("Settings")
            .default_pos([200.0, 120.0])
            .show(&context, |ui| {
                ui.checkbox(&mut self.enabled, "Enabled");
                ui.add(Slider::new(&mut self.volume, 0.0..=1.0).text("Volume"));
                ui.add(ProgressBar::new(0.6).show_percentage());
                ui.separator();
                ui.monospace("The quick brown fox jumps over the lazy dog 0123456789");
            });

        Window::new("Log")
            .default_pos([320.0, 240.0])
            .show(&context, |ui| {
                ScrollArea::vertical().max_height(200.0).show(ui, |ui| {
                    for index in 0..32 {
                        ui.label(format!("[{index:03}] Lorem ipsum dolor sit amet"));
                    }
                });
            });
    }
}

fn render<
    I: Intermediate,
    P: From<Srgba<u8>> + Into<Srgba<u8>> + Send + Sync + Copy + 'static,
    const W: usize,
    const H: usize,
    const BATCH_SIZE: usize,
>(
    renderer: &mut Renderer<I>,
    context: &Context,
    scene: &mut Scene,
    mut texture: impl AsViewTextureMut<P>,
) {
    texture.as_view_mut().fill(BLACK.with_alpha(u8::MAX).into());

    let full_output = context.run_ui(
        RawInput {
            viewport_id: ViewportId::ROOT,
            viewports: HashMap::from_iter([(
                ViewportId::ROOT,
                ViewportInfo {
                    focused: Some(true),
                    fullscreen: Some(true),
                    ..Default::default()
                },
            )]),
            screen_rect: Some(Rect::from_min_max(
                [0.0, 0.0].into(),
                [W as f32, H as f32].into(),
            )),
            ..Default::default()
        },
        |ui| {
            scene.ui(ui);
        },
    );

    renderer.render::<P, BATCH_SIZE>(context, full_output, texture);
}

// Combinational macro to save copy pastes
macro_rules! bench_combinations {
    ($group:ident, $numbers:tt, [$(($w:expr, $h:expr)),+ $(,)?], $formats:tt, $batches:tt) => {
        $(
            bench_combinations!(@expand_size $group, $numbers, ($w, $h), $formats, $batches);
        )+
    };
    (@expand_size $group:ident, [$($number_name:literal => $number:ty),+ $(,)?], ($w:expr, $h:expr), $formats:tt, $batches:tt) => {
        $(
            bench_combinations!(@expand_number $group, ($number_name, $number), ($w, $h), $formats, $batches);
        )+
    };
    (@expand_number $group:ident, $number:tt, ($w:expr, $h:expr), [$($name:literal => $t:ty),+ $(,)?], $batches:tt) => {
        $(
            bench_combinations!(@expand_format $group, $number, ($w, $h), ($name, $t), $batches);
        )+
    };
    (@expand_format $group:ident, ($number_name:literal, $number:ty), ($w:expr, $h:expr), ($name:literal, $t:ty), [$($batch:expr),+ $(,)?]) => {
        $(
            {
                let mut renderer = Renderer::<$number>::default();
                let context = setup_context();
                let mut texture = OwnedTexture::from_value($w, $h, BLACK.with_alpha(u8::MAX).into());

                let bench_name = format!("{}x{}", $w, $h);
                let param_combo = format!("{} {} @ Batch Size {}", $number_name, $name, $batch);

                let mut scene = Scene::default();

                $group.bench_function(BenchmarkId::new(bench_name, param_combo), |b| {
                    b.iter(|| {
                        render::<$number, $t, $w, $h, $batch>(
                            &mut renderer,
                            &context,
                            &mut scene,
                            black_box(texture.as_view_mut()),
                        )
                    });
                });
            }
        )+
    };
}

fn bench_egui_software(c: &mut Criterion) {
    let mut group = c.benchmark_group(format!("{}/egui_software", env!("CARGO_PKG_NAME")));

    bench_combinations!(
        group,
        [
            "f32" => f32,
            "f64" => f64,
            "Q16.16" => I16F16,
            "Q32.32" => I32F32,
        ],
        // 3 most common screen sizes to my knowledge
        [
            (640, 480),
            (1280, 720),
            (1920, 1080),
        ],
        // The 2 pixel orderings we care about
        [
            "rgba" => Packed<Rgba, [u8; 4]>,
            "bgra" => Packed<Bgra, [u8; 4]>,
        ],
        // Relevant batch widths to force to
        [1, 2, 4, 8, 16, 32]
    );

    group.finish();
}

criterion_group!(benches, bench_egui_software);
criterion_main!(benches);
