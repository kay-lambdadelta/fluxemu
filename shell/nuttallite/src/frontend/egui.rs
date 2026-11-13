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
use palette::named::BLACK;
use tokio::runtime::Handle;

use crate::{
    build_machine::get_software_factories,
    platform::Platform,
    runtime::{AudioRuntime, GraphicsRuntime, open_graphics_runtime},
};

const UNSCII_FONT: &[u8] = include_bytes!("../../../../external/unscii/fontfiles/unscii-16.otf");

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
        get_software_factories(),
        program_manager,
        program_database,
        AudioRuntime,
        None,
    );
    let mut frontend = Frontend::new(core, notifications, font_definitions);
    let mut graphics_runtime = open_graphics_runtime()?;

    let display_dimensions = graphics_runtime.display_context().dimensions();

    let start_time = Instant::now();

    loop {
        frontend.maybe_reset_graphics_to_meet_machine_requirements(|_, sealed_machine_builder| {
            graphics_runtime.reconfigure(sealed_machine_builder.graphics_requirements());
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
                max: [display_dimensions.x as f32, display_dimensions.y as f32].into(),
            }),
            time: Some(start_time.elapsed().as_secs_f64()),
            focused: true,
            ..Default::default()
        };

        let full_output = frontend.run(raw_input, &mut graphics_runtime);

        graphics_runtime.present(frontend.egui_context(), BLACK, full_output);
    }
}
