use divan::Bencher;
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
use std::collections::HashMap;

fn main() {
    divan::main();
}

fn render<const W: usize, const H: usize, P: From<Srgba<u8>> + Into<Srgba<u8>> + Copy + 'static>(
    renderer: &mut Renderer,
    context: &Context,
) {
    let mut texture = Texture::new(W, H, BLACK.with_alpha(u8::MAX).into());
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
    renderer.render::<P>(context, full_output.clone(), texture.as_view_mut());
}

#[divan::bench]
fn egui_software_1280x720_rgba(bencher: Bencher) {
    let mut renderer = Renderer::default();
    let context = Context::default();

    bencher.bench_local(|| {
        render::<1280, 720, Packed<Rgba, [u8; 4]>>(&mut renderer, &context);
    });
}

#[divan::bench]
fn egui_software_1280x720_bgra(bencher: Bencher) {
    let mut renderer = Renderer::default();
    let context = Context::default();

    bencher.bench_local(|| {
        render::<1280, 720, Packed<Bgra, [u8; 4]>>(&mut renderer, &context);
    });
}

#[divan::bench]
fn egui_software_1920x1080_rgba(bencher: Bencher) {
    let mut renderer = Renderer::default();
    let context = Context::default();

    bencher.bench_local(|| {
        render::<1920, 1080, Packed<Rgba, [u8; 4]>>(&mut renderer, &context);
    });
}

#[divan::bench]
fn egui_software_1920x1080_bgra(bencher: Bencher) {
    let mut renderer = Renderer::default();
    let context = Context::default();

    bencher.bench_local(|| {
        render::<1920, 1080, Packed<Bgra, [u8; 4]>>(&mut renderer, &context);
    });
}
