use std::{collections::HashMap, sync::Arc, time::Instant};

use egui::{FontData, FontDefinitions, FontFamily, RawInput, Rect, ViewportId, ViewportInfo};
use fluxemu_database::ProgramDatabase;
use fluxemu_environment::Environment;
use fluxemu_frontend::{
    FrontendCore,
    graphics::{DisplayContext as _, GraphicsRuntime as _},
    notification::NotificationReceiver,
};
use fluxemu_frontend_egui::{Frontend, rendering::EguiCapableGraphicsRuntime};
use fluxemu_program::ProgramManager;
use nalgebra::Vector2;
use palette::named::BLACK;
use tokio::runtime::Handle;

use crate::{
    build_machine::get_factories,
    platform::Platform,
    runtime::{AudioRuntime, GraphicsRuntime, Touchscreen, open_graphics_runtime},
};

// Many test screens are entirely too small and need this to keep anything visible
const MINIMUM_LOGICAL_DIMENSIONS: Vector2<f32> = Vector2::new(320.0, 240.0);
const UNSCII_FONT: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/unscii-16.otf"));

pub fn run(
    runtime: Handle,
    environment: Environment,
    program_manager: Arc<ProgramManager>,
    program_database: Arc<ProgramDatabase>,
    notifications: NotificationReceiver,
) -> Result<(), Box<dyn std::error::Error>> {
    let font_definitions = FontDefinitions {
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
    };
    let core = FrontendCore::<Platform<GraphicsRuntime>>::new(
        runtime,
        environment,
        get_factories(),
        program_manager,
        program_database,
        AudioRuntime,
        None,
    );
    let mut frontend = Frontend::new(core, notifications, font_definitions);
    let mut graphics_runtime = open_graphics_runtime()?;
    let mut touchscreen = Touchscreen::open();

    let display_dimensions = graphics_runtime
        .display_context()
        .dimensions()
        .cast::<f32>();
    let pixels_per_point = display_dimensions
        .component_div(&MINIMUM_LOGICAL_DIMENSIONS)
        .min()
        .min(1.0);
    let logical_dimensions = display_dimensions / pixels_per_point;

    let start_time = Instant::now();

    loop {
        frontend.maybe_reset_graphics_to_meet_machine_requirements(|_, sealed_machine_builder| {
            graphics_runtime.reconfigure(sealed_machine_builder.graphics_requirements());
        });

        let mut events = Vec::new();

        if let Some(touchscreen) = &mut touchscreen {
            touchscreen.poll(&mut events, pixels_per_point);
        }

        let raw_input = RawInput {
            viewport_id: ViewportId::ROOT,
            viewports: HashMap::from_iter([(
                ViewportId::ROOT,
                ViewportInfo {
                    focused: Some(true),
                    fullscreen: Some(true),
                    native_pixels_per_point: Some(pixels_per_point),
                    ..Default::default()
                },
            )]),
            screen_rect: Some(Rect {
                min: [0.0, 0.0].into(),
                max: [logical_dimensions.x, logical_dimensions.y].into(),
            }),
            time: Some(start_time.elapsed().as_secs_f64()),
            max_texture_side: Some(1024),
            events,
            focused: true,
            ..Default::default()
        };

        let full_output = frontend.run(raw_input, &mut graphics_runtime);

        graphics_runtime.present(frontend.egui_context(), BLACK, full_output);
    }
}
