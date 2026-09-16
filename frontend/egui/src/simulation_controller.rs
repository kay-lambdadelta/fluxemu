use std::time::Duration;

use egui::{Label, Response, ScrollArea, Sense, Ui, Widget};
use egui_extras::{Column, TableBuilder};
use fluxemu_frontend::simulation_controller::{Controller, JITTER_CEILING};
use rust_i18n::t;

#[derive(Debug)]
pub struct State {
    execution_timeslice: f32,
    target_timeslice: f32,
    hardware_speed_ema: f32,
    jitter_ratio: f32,
    update_rate: Duration,
}

impl Default for State {
    fn default() -> Self {
        Self {
            execution_timeslice: 0.0,
            target_timeslice: 0.0,
            hardware_speed_ema: 0.0,
            jitter_ratio: 0.0,
            update_rate: Duration::from_millis(200),
        }
    }
}

impl State {
    pub fn update(&mut self, simulation_controller: &Controller) {
        let state = simulation_controller.get_state_snapshot();

        self.execution_timeslice = state.execution_timeslice;
        self.target_timeslice = state.target_timeslice;
        self.hardware_speed_ema = state.hardware_speed_ema;
        self.jitter_ratio = state.jitter_ratio;
    }
}

impl Widget for &mut State {
    fn ui(self, ui: &mut Ui) -> Response {
        ui.ctx().request_repaint_after(self.update_rate);

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
                        format!("{:?}", Duration::from_secs_f32(self.execution_timeslice)),
                    );
                    stat_row(
                        t!("simulation_controller.target_timeslice"),
                        format!("{:?}", Duration::from_secs_f32(self.target_timeslice)),
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
                });
        });

        ui.allocate_rect(ui.min_rect(), Sense::empty())
    }
}
