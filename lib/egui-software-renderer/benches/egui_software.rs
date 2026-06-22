use std::{collections::HashMap, hint::black_box};

use criterion::{BenchmarkId, Criterion, criterion_group, criterion_main};
use egui::{Context, RawInput, Rect, ViewportId, ViewportInfo};
use fluxemu_egui_software_renderer::Renderer;
use fluxemu_graphics::api::software::texture::{Texture, TextureImplMut};
use palette::{
    Srgba, WithAlpha,
    cast::Packed,
    named::BLACK,
    rgb::channels::{Bgra, Rgba},
};

fn render<
    P: From<Srgba<u8>> + Into<Srgba<u8>> + Send + Sync + Copy + 'static,
    const W: usize,
    const H: usize,
    const BATCH_SIZE: usize,
>(
    renderer: &mut Renderer,
    context: &Context,
    texture: &mut Texture<P>,
) {
    texture.fill(BLACK.with_alpha(u8::MAX).into());

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
            for _ in 0..25 {
                ui.vertical_centered(|ui| {
                    let _ = ui.button(
                        "Lorem ipsum dolor sit amet, consectetur adipiscing elit, sed do eiusmod \
                         tempor incididunt ut labore et dolore magna aliqua. Ut enim ad minim \
                         veniam, quis nostrud exercitation ullamco laboris nisi ut aliquip ex ea \
                         commodo consequat. Duis aute irure dolor in reprehenderit in voluptate \
                         velit esse cillum dolore eu fugiat nulla pariatur. Excepteur sint \
                         occaecat cupidatat non proident, sunt in culpa qui officia deserunt \
                         mollit anim id est laborum.",
                    );
                });
            }
        },
    );

    renderer.render::<P, BATCH_SIZE>(context, full_output, texture);
}

// Combinational macro to save copy pastes
macro_rules! bench_combinations {
    ($group:ident, [$(($w:expr, $h:expr)),+ $(,)?], $formats:tt, $batches:tt) => {
        $(
            bench_combinations!(@expand_size $group, ($w, $h), $formats, $batches);
        )+
    };
    (@expand_size $group:ident, ($w:expr, $h:expr), [$($name:literal => $t:ty),+ $(,)?], $batches:tt) => {
        $(
            bench_combinations!(@expand_format $group, ($w, $h), ($name, $t), $batches);
        )+
    };
    (@expand_format $group:ident, ($w:expr, $h:expr), ($name:literal, $t:ty), [$($batch:expr),+ $(,)?]) => {
        $(
            {
                let mut renderer = Renderer::default();
                let context = Context::default();
                let mut texture = Texture::new($w, $h, BLACK.with_alpha(u8::MAX).into());

                let bench_name = format!("{}x{}", $w, $h);
                let param_combo = format!("{} @ Batch Size {}", $name, $batch);

                $group.bench_function(BenchmarkId::new(bench_name, param_combo), |b| {
                    b.iter(|| {
                        render::<$t, $w, $h, $batch>(
                            &mut renderer,
                            &context,
                            black_box(&mut texture),
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
