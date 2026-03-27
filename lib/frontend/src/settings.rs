use std::ops::Deref;

use egui::{ComboBox, RichText};
use egui_material_icons::icons::ICON_SAVE;
use fluxemu_environment::{ENVIRONMENT_LOCATION, graphics::GraphicsApi};
use ron::ser::PrettyConfig;
use strum::IntoEnumIterator;

use crate::{Frontend, FrontendPlatform};

impl<P: FrontendPlatform> Frontend<P> {
    pub fn handle_settings(&mut self, ui: &mut egui::Ui) {
        ui.horizontal_top(|ui| {
            let button_text = RichText::new(ICON_SAVE).size(32.0);

            if ui
                .button(button_text)
                .on_hover_text("Save environment to disk")
                .clicked()
            {
                let environment_string =
                    ron::ser::to_string_pretty(&self.environment, PrettyConfig::default()).unwrap();

                std::thread::spawn(|| {
                    if let Err(err) =
                        std::fs::write(ENVIRONMENT_LOCATION.deref(), environment_string)
                    {
                        tracing::error!("Failed to save environment: {}", err);
                    }
                });
            }
        });

        ui.separator();

        ComboBox::from_label("Graphics Api")
            .selected_text(self.environment.graphics_setting.api.to_string())
            .show_ui(ui, |ui| {
                for api in GraphicsApi::iter() {
                    ui.selectable_value(
                        &mut self.environment.graphics_setting.api,
                        api,
                        api.to_string(),
                    );
                }
            });
    }
}
