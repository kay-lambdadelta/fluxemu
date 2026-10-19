use std::{collections::HashMap, sync::Arc, time::Instant};

use egui::{FontData, FontDefinitions, FontFamily, RawInput, Rect, ViewportId, ViewportInfo};
use fluxemu_environment::Environment;
use fluxemu_frontend::{
    FrontendCore, graphics::GraphicsRuntime as _, notification::NotificationReceiver,
};
use fluxemu_frontend_egui::{Frontend, rendering::EguiCapableGraphicsRuntime};
use fluxemu_program::ProgramManager;
use palette::named::BLACK;

use crate::{
    build_machine::get_software_factories,
    platform::Platform,
    runtime::{AudioRuntime, GraphicsRuntime},
};

const UNSCII_FONT: &[u8] = include_bytes!("../../../../external/unscii/fontfiles/unscii-16.otf");

pub fn run(
    environment: Environment,
    program_manager: Arc<ProgramManager>,
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
        environment,
        get_software_factories(),
        program_manager,
        AudioRuntime,
        None,
    );
    let mut frontend = Frontend::new(core, notifications, font_definitions);

    let mut graphics_runtime = GraphicsRuntime::default();

    let start_time = Instant::now();

    loop {
        frontend.maybe_reset_graphics_to_meet_machine_requirements(|_, sealed_machine_builder| {
            graphics_runtime.reconfigure(sealed_machine_builder.graphics_requirements());
            graphics_runtime.component_initialization_data()
        });

        let raw_input = RawInput {
            viewport_id: ViewportId::ROOT,
            viewports: HashMap::from_iter([(
                ViewportId::ROOT,
                ViewportInfo {
                    focused: Some(true),
                    fullscreen: Some(true),
                    native_pixels_per_point: Some(1.0),
                    ..Default::default()
                },
            )]),
            screen_rect: Some(Rect {
                min: [0.0, 0.0].into(),
                max: [
                    graphics_runtime.texture.width() as f32,
                    graphics_runtime.texture.height() as f32,
                ]
                .into(),
            }),
            time: Some(start_time.elapsed().as_secs_f64()),
            focused: true,
            ..Default::default()
        };

        let full_output = frontend.run(raw_input, &mut graphics_runtime);

        graphics_runtime.present(frontend.egui_context(), BLACK, full_output);
    }
}
