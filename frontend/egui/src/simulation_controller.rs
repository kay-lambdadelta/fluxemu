use std::time::{Duration, Instant};

use egui::{Area, Context, Frame, Grid, Id, Order, Pos2, RichText, TextStyle, Ui};
use fluxemu_frontend::simulation_controller::{Controller, JITTER_CEILING};
use palette::{
    WithAlpha,
    named::{BLACK, ORANGE, WHITE},
};
use rust_i18n::t;

use crate::utils::to_egui_color;

#[derive(Debug)]
pub struct State {
    execution_timeslice: f32,
    target_timeslice: f32,
    hardware_speed_ema: f32,
    jitter_ratio: f32,
    update_rate: Duration,
    last_update: Option<Instant>,
}

impl Default for State {
    fn default() -> Self {
        Self {
            execution_timeslice: 0.0,
            target_timeslice: 0.0,
            hardware_speed_ema: 0.0,
            jitter_ratio: 0.0,
            update_rate: Duration::from_millis(200),
            last_update: None,
        }
    }
}

impl State {
    pub fn update(&mut self, simulation_controller: &Controller) {
        if self
            .last_update
            .is_some_and(|last_update| last_update.elapsed() < self.update_rate)
        {
            return;
        }

        let state = simulation_controller.get_state_snapshot();

        self.execution_timeslice = state.execution_timeslice;
        self.target_timeslice = state.target_timeslice;
        self.hardware_speed_ema = state.hardware_speed_ema;
        self.jitter_ratio = state.jitter_ratio;
        self.last_update = Some(Instant::now());
    }

    pub fn show_overlay(&self, context: &Context, position: Pos2) {
        context.request_repaint_after(self.update_rate);

        Area::new(Id::unique("debug_overlay"))
            .order(Order::Foreground)
            .fixed_pos(position)
            .interactable(false)
            .show(context, |ui| {
                Frame::NONE
                    .fill(to_egui_color(BLACK.with_alpha(220)))
                    .inner_margin(8.0)
                    .corner_radius(4.0)
                    .show(ui, |ui| {
                        ui.style_mut().override_text_style = Some(TextStyle::Monospace);

                        Grid::new("debug_overlay_stats")
                            .num_columns(2)
                            .show(ui, |ui| self.show_stats(ui));
                    });
            });
    }

    fn show_stats(&self, ui: &mut Ui) {
        let mut stat_row = |label: std::borrow::Cow<'_, str>, value: String| {
            ui.label(RichText::new(label).color(to_egui_color(ORANGE.with_alpha(u8::MAX))));
            ui.label(RichText::new(value).color(to_egui_color(WHITE.with_alpha(u8::MAX))));
            ui.end_row();
        };

        stat_row(
            t!("simulation_controller.execution_timeslice"),
            format!("{:.2?}", Duration::from_secs_f32(self.execution_timeslice)),
        );
        stat_row(
            t!("simulation_controller.target_timeslice"),
            format!("{:.2?}", Duration::from_secs_f32(self.target_timeslice)),
        );
        stat_row(
            t!("simulation_controller.hardware_speed"),
            format!("{:.1}%", self.hardware_speed_ema * 100.0),
        );
        stat_row(
            t!("simulation_controller.jitter_ratio"),
            format!(
                "{:.1}% (ceiling {:.1}%)",
                self.jitter_ratio * 100.0,
                JITTER_CEILING * 100.0
            ),
        );
    }
}
