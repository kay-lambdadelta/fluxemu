use std::{collections::HashMap, hint::black_box};

use criterion::{Criterion, criterion_group, criterion_main};
use egui::{Context, FullOutput, RawInput, TopBottomPanel, ViewportId, ViewportInfo};
use fluxemu_frontend::gui_software_rendering::SoftwareEguiRenderer;
use nalgebra::{DMatrix, Vector2};
use palette::{
    cast::Packed,
    named::BLACK,
    rgb::channels::{Bgra, Rgba},
};

fn criterion_benchmark(c: &mut Criterion) {
    let mut group = c.benchmark_group(env!("CARGO_PKG_NAME"));

    let mut renderer = SoftwareEguiRenderer::default();

    for resolution in [
        Vector2::new(640, 480),
        Vector2::new(1280, 720),
        Vector2::new(1920, 1080),
    ] {
        let (context, full_output) =
            setup_output(Vector2::new(resolution.x as f32, resolution.y as f32));

        let mut texture = black_box(DMatrix::from_element(
            resolution.x,
            resolution.y,
            Packed::pack(BLACK.into()),
        ));
        group.bench_function(
            format!("software_rendering_{}x{}_bgra", resolution.x, resolution.y),
            |b| {
                b.iter_with_large_drop(|| {
                    renderer.render::<Bgra>(&context, texture.as_view_mut(), full_output.clone());
                })
            },
        );

        let mut texture = black_box(DMatrix::from_element(
            resolution.x,
            resolution.y,
            Packed::pack(BLACK.into()),
        ));
        group.bench_function(
            format!("software_rendering_{}x{}_rgba", resolution.x, resolution.y),
            |b| {
                b.iter_with_large_drop(|| {
                    renderer.render::<Rgba>(&context, texture.as_view_mut(), full_output.clone());
                })
            },
        );
    }
}

fn setup_output(resolution: Vector2<f32>) -> (Context, FullOutput) {
    let context = Context::default();

    let full_output = context.run(
        RawInput {
            viewport_id: ViewportId::ROOT,
            viewports: HashMap::from_iter([(
                ViewportId::ROOT,
                ViewportInfo {
                    native_pixels_per_point: Some(1.0),
                    monitor_size: Some([resolution.x, resolution.y].into()),
                    ..Default::default()
                },
            )]),
            focused: true,
            ..Default::default()
        },
        |ctx| {
            TopBottomPanel::top("panel")
                .resizable(true)
                .show(ctx, |ui| {
                    // Create some labels
                    for i in 0..100 {
                        ui.label(i.to_string());
                    }
                });
        },
    );

    (context, full_output)
}

criterion_group!(benches, criterion_benchmark);
criterion_main!(benches);
