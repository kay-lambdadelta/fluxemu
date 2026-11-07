use egui::{ComboBox, Grid, ScrollArea, Slider, Ui, WidgetText};
use fluxemu_environment::graphics::GraphicsApi;
use fluxemu_frontend::Platform;
use fluxemu_graphics::shader::ShaderType;
use rust_i18n::t;
use strum::IntoEnumIterator;

use crate::Frontend;

impl<P: Platform> Frontend<P> {
    pub fn handle_settings(&mut self, ui: &mut Ui) {
        ui.horizontal_top(|ui| {
            if ui
                .button(t!("settings.save"))
                .on_hover_text(t!("settings.save_hint"))
                .clicked()
            {
                self.core.save_environment();
            }
        });
        ui.separator();

        ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
            section(ui, t!("settings.graphics"), |ui| {
                Grid::new("graphics_settings")
                    .num_columns(2)
                    .spacing([24.0, 8.0])
                    .show(ui, |ui| {
                        let environment = self.core.environment_mut();

                        ui.label(t!("settings.graphics_api"))
                            .on_hover_text(t!("settings.requires_restart"));
                        ComboBox::from_id_salt("graphics_api")
                            .selected_text(environment.graphics.api.to_string())
                            .show_ui(ui, |ui| {
                                for api in GraphicsApi::iter() {
                                    ui.selectable_value(
                                        &mut environment.graphics.api,
                                        api,
                                        api.to_string(),
                                    );
                                }
                            });
                        ui.end_row();

                        ui.label(t!("settings.shader"));
                        ComboBox::from_id_salt("shader")
                            .selected_text(environment.graphics.shader.to_string())
                            .show_ui(ui, |ui| {
                                for shader in ShaderType::iter() {
                                    ui.selectable_value(
                                        &mut environment.graphics.shader,
                                        shader,
                                        shader.to_string(),
                                    );
                                }
                            });
                        ui.end_row();
                    });
            });

            section(ui, t!("settings.audio"), |ui| {
                Grid::new("audio_settings")
                    .num_columns(2)
                    .spacing([24.0, 8.0])
                    .show(ui, |ui| {
                        let old_volume = self.core.environment().audio.volume;
                        let mut volume = old_volume;

                        ui.label(t!("settings.volume"));
                        ui.add(
                            Slider::new(&mut volume, 0.0..=1.0)
                                .custom_formatter(|value, _| format!("{:.0}%", value * 100.0))
                                .custom_parser(|text| {
                                    text.replace('%', "")
                                        .trim()
                                        .parse::<f64>()
                                        .map(|percentage| percentage / 100.0)
                                        .ok()
                                }),
                        );
                        ui.end_row();

                        if volume != old_volume {
                            self.core.set_volume(volume);
                        }
                    });
            });

            section(ui, t!("rom_stores.heading"), |ui| {
                self.rom_store_editor.show(ui, &mut self.core);
            });
        });
    }
}

fn section(ui: &mut Ui, title: impl Into<WidgetText>, add_contents: impl FnOnce(&mut Ui)) {
    ui.label(title.into().strong());
    add_contents(ui);
    ui.separator();
}
