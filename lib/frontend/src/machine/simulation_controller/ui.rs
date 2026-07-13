use std::time::Duration;

use egui::{Label, Response, ScrollArea, Sense, Ui, Widget};
use egui_extras::{Column, TableBuilder};
use rust_i18n::t;

use crate::machine::{
    SimulationController,
    simulation_controller::{JITTER_CEILING, UI_UPDATE_RATE},
};

#[derive(Debug)]
pub struct UiState {
    execution_timeslice: f32,
    target_timeslice: f32,
    hardware_speed_ema: f32,
    jitter_ratio: f32,
}

impl Default for UiState {
    fn default() -> Self {
        Self {
            execution_timeslice: 0.0,
            target_timeslice: 0.0,
            hardware_speed_ema: 0.0,
            jitter_ratio: 0.0,
        }
    }
}

impl Widget for &mut SimulationController {
    fn ui(self, ui: &mut Ui) -> Response {
        let state = self.shared.state.lock().unwrap();

        self.ui_state.execution_timeslice = state.execution_timeslice;
        self.ui_state.target_timeslice = state.target_timeslice;
        self.ui_state.hardware_speed_ema = state.hardware_speed_ema;
        self.ui_state.jitter_ratio = state.jitter_ratio;

        drop(state);

        ui.ctx().request_repaint_after(UI_UPDATE_RATE);

        ScrollArea::vertical().show(ui, |ui| {
            TableBuilder::new(ui)
                .column(Column::auto().resizable(true))
                .column(Column::remainder())
                .striped(true)
                .body(|mut body| {
                    let mut stat_row = |label, value| {
                        body.row(30.0, |mut row| {
                            row.col(|ui| {
                                ui.add(Label::new(label).extend());
                            });
                            row.col(|ui| {
                                ui.label(value);
                            });
                        });
                    };

                    stat_row(
                        t!("simulation_controller.execution_timeslice"),
                        format!(
                            "{:?}",
                            Duration::from_secs_f32(self.ui_state.execution_timeslice)
                        ),
                    );
                    stat_row(
                        t!("simulation_controller.target_timeslice"),
                        format!(
                            "{:?}",
                            Duration::from_secs_f32(self.ui_state.target_timeslice)
                        ),
                    );
                    stat_row(
                        t!("simulation_controller.hardware_speed"),
                        format!("{:.1}%", self.ui_state.hardware_speed_ema * 100.0),
                    );
                    stat_row(
                        t!("simulation_controller.jitter_ratio"),
                        format!(
                            "{:.1}% (ceiling {:.1}%)",
                            self.ui_state.jitter_ratio * 100.0,
                            JITTER_CEILING * 100.0
                        ),
                    );
                });
        });

        ui.allocate_rect(ui.min_rect(), Sense::empty())
    }
}
