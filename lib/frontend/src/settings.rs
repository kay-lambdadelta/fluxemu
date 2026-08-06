use egui::{ComboBox, RichText, Slider};
use egui_material_icons::icons::{ICON_SAVE, ICON_VOLUME_OFF, ICON_VOLUME_UP};
use fluxemu_environment::graphics::GraphicsApi;
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
                let environment_location = self.user_environment_location.clone();

                let environment_string =
                    ron::ser::to_string_pretty(&self.environment, PrettyConfig::default()).unwrap();

                std::thread::spawn(|| {
                    if let Err(err) = std::fs::write(environment_location, environment_string) {
                        tracing::error!("Failed to save environment: {}", err);
                    }
                });
            }
        });

        ComboBox::from_label("Graphics Api")
            .selected_text(self.environment.graphics.api.to_string())
            .show_ui(ui, |ui| {
                for api in GraphicsApi::iter() {
                    ui.selectable_value(&mut self.environment.graphics.api, api, api.to_string());
                }
            });

        ui.horizontal(|ui| {
            let old_volume = self.environment.audio.volume;

            let text = if self.environment.audio.volume == 0.0 {
                format!("Global Volume {}", ICON_VOLUME_OFF.codepoint)
            } else {
                format!("Global Volume {}", ICON_VOLUME_UP.codepoint)
            };

            ui.add(
                Slider::new(&mut self.environment.audio.volume, 0.0..=1.0)
                    .text(text)
                    .custom_formatter(|value, _| format!("{:.0}%", value * 100.0))
                    .custom_parser(|s| {
                        s.replace('%', "")
                            .trim()
                            .parse::<f64>()
                            .map(|p| p / 100.0)
                            .ok()
                    }),
            );

            if self.environment.audio.volume != old_volume {
                self.audio_mixer.set_volume(self.environment.audio.volume);
            }
        });
    }
}
