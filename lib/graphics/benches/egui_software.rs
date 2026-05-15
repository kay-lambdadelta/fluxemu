use std::collections::HashMap;

use criterion::{BenchmarkId, Criterion, criterion_group, criterion_main};
use egui::{Context, RawInput, Rect, ViewportId, ViewportInfo};
use fluxemu_graphics::api::software::{
    egui_renderer::Renderer,
    texture::{Texture, TextureImplMut},
};
use palette::{
    Srgba, WithAlpha,
    cast::Packed,
    named::BLACK,
    rgb::channels::{Bgra, Rgba},
};

fn render<const W: usize, const H: usize, P: From<Srgba<u8>> + Into<Srgba<u8>> + Copy + 'static>(
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
            for _ in 0..10 {
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

    renderer.render::<P>(context, full_output, texture.as_view_mut());
}

fn bench_egui_software(c: &mut Criterion) {
    let mut group = c.benchmark_group(format!("{}/egui_software", env!("CARGO_PKG_NAME")));

    {
        let mut renderer = Renderer::default();
        let context = Context::default();
        let mut texture = Texture::new(1280, 720, BLACK.with_alpha(u8::MAX).into());

        group.bench_function(BenchmarkId::new("1280x720", "rgba"), |b| {
            b.iter(|| {
                render::<1280, 720, Packed<Rgba, [u8; 4]>>(&mut renderer, &context, &mut texture)
            });
        });
    }

    {
        let mut renderer = Renderer::default();
        let context = Context::default();
        let mut texture = Texture::new(1280, 720, BLACK.with_alpha(u8::MAX).into());

        group.bench_function(BenchmarkId::new("1280x720", "bgra"), |b| {
            b.iter(|| {
                render::<1280, 720, Packed<Bgra, [u8; 4]>>(&mut renderer, &context, &mut texture)
            });
        });
    }

    {
        let mut renderer = Renderer::default();
        let context = Context::default();
        let mut texture = Texture::new(1920, 1080, BLACK.with_alpha(u8::MAX).into());

        group.bench_function(BenchmarkId::new("1920x1080", "rgba"), |b| {
            b.iter(|| {
                render::<1920, 1080, Packed<Rgba, [u8; 4]>>(&mut renderer, &context, &mut texture)
            });
        });
    }

    {
        let mut renderer = Renderer::default();
        let context = Context::default();
        let mut texture = Texture::new(1920, 1080, BLACK.with_alpha(u8::MAX).into());

        group.bench_function(BenchmarkId::new("1920x1080", "bgra"), |b| {
            b.iter(|| {
                render::<1920, 1080, Packed<Bgra, [u8; 4]>>(&mut renderer, &context, &mut texture)
            });
        });
    }

    group.finish();
}

criterion_group!(benches, bench_egui_software);
criterion_main!(benches);
