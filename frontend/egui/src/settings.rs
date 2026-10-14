use egui::{ComboBox, RichText, Slider};
use fluxemu_environment::graphics::GraphicsApi;
use fluxemu_frontend::Platform;
use strum::IntoEnumIterator;

use crate::Frontend;

impl<P: Platform> Frontend<P> {
    pub fn handle_settings(&mut self, ui: &mut egui::Ui) {
        ui.horizontal_top(|ui| {
            let button_text = RichText::new("💾").size(32.0);

            if ui
                .button(button_text)
                .on_hover_text("Save environment to disk")
                .clicked()
            {
                self.core.save_environment();
            }
        });

        let environment = self.core.environment_mut();

        ComboBox::from_label("Graphics Api")
            .selected_text(environment.graphics.api.to_string())
            .show_ui(ui, |ui| {
                for api in GraphicsApi::iter() {
                    ui.selectable_value(&mut environment.graphics.api, api, api.to_string());
                }
            });

        ui.horizontal(|ui| {
            let old_volume = self.core.environment().audio.volume;
            let mut volume = old_volume;

            let text = if volume == 0.0 {
                "Global Volume 🔇"
            } else {
                "Global Volume 🔊"
            };

            ui.add(
                Slider::new(&mut volume, 0.0..=1.0)
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

            if volume != old_volume {
                self.core.set_volume(volume);
            }
        });
    }
}
