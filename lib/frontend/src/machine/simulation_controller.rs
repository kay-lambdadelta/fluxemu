use std::{
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    thread::{JoinHandle, sleep},
    time::{Duration, Instant},
};

use egui::{Label, Response, Sense, Ui, Widget};
use egui_extras::{Column, TableBuilder};
use fluxemu_runtime::machine::Machine;
use ringbuffer::{ConstGenericRingBuffer, RingBuffer};
use rust_i18n::t;

use crate::AudioMixer;

const OUTLIER_MULTIPLE: f32 = 10.0;
const GROWTH_DIVISOR: f32 = 8.0;
const SHRINK_DIVISOR: f32 = 4.0;

#[derive(Debug)]
pub struct SimulationController {
    shared: Arc<SharedState>,
    handle: Option<JoinHandle<()>>,
}

impl Drop for SimulationController {
    fn drop(&mut self) {
        self.shared.should_exit.store(true, Ordering::Release);

        let handle = self.handle.take().unwrap();
        handle.thread().unpark();

        handle.join().unwrap();
    }
}

impl SimulationController {
    pub fn new(machine: Arc<Machine>, audio_mixer: Arc<AudioMixer>) -> Self {
        let shared = Arc::new(SharedState {
            paused: AtomicBool::new(true),
            should_exit: AtomicBool::new(false),
            simulation_controller_state: Mutex::new(SimulationControllerState {
                execution_timeslice: Duration::from_millis(4).as_secs_f32(),
                sleep_threshold: Duration::from_millis(1).as_secs_f32(),
                error: 0.0,
                execution_time_ring: ConstGenericRingBuffer::default(),
                jitter_ring: ConstGenericRingBuffer::default(),
                sleep_overshoot_ring: ConstGenericRingBuffer::default(),
                jitter_ratio: 0.0,
            }),
        });

        let handle = std::thread::Builder::new()
            .name("simulation_controller".to_string())
            .spawn({
                let shared = shared.clone();

                move || {
                    simulation_controller_loop(machine, audio_mixer, shared);
                }
            })
            .expect("Failed to spawn simulation controller thread");

        Self {
            shared,
            handle: Some(handle),
        }
    }

    pub fn set_paused(&self, paused: bool) {
        self.shared.paused.store(paused, Ordering::Release);

        self.handle.as_ref().unwrap().thread().unpark();
    }

    pub fn snapshot_simulation_controller_state(&self) -> SimulationControllerState {
        self.shared
            .simulation_controller_state
            .lock()
            .unwrap()
            .clone()
    }
}

#[derive(Debug)]
struct SharedState {
    paused: AtomicBool,
    should_exit: AtomicBool,
    simulation_controller_state: Mutex<SimulationControllerState>,
}

#[derive(Debug, Clone)]
pub struct SimulationControllerState {
    pub execution_timeslice: f32,
    pub sleep_threshold: f32,
    pub error: f32,
    pub jitter_ratio: f32,

    pub execution_time_ring: ConstGenericRingBuffer<f32, 16>,
    pub jitter_ring: ConstGenericRingBuffer<f32, 32>,
    pub sleep_overshoot_ring: ConstGenericRingBuffer<f32, 16>,
}

fn simulation_controller_loop(
    machine: Arc<Machine>,
    audio_mixer: Arc<AudioMixer>,
    shared: Arc<SharedState>,
) {
    loop {
        if shared.should_exit.load(Ordering::Acquire) {
            break;
        }

        if shared.paused.load(Ordering::Acquire) {
            std::thread::park();

            continue;
        }

        let mut simulation_controller_state_guard =
            shared.simulation_controller_state.lock().unwrap();

        let start = Instant::now();

        // Enter runtime
        let runtime_guard = machine.enter_runtime();

        // Run simulation
        runtime_guard.run_duration(Duration::from_secs_f32(
            simulation_controller_state_guard.execution_timeslice,
        ));

        // Extract audio samples
        audio_mixer.extract_machine_samples(&runtime_guard);

        // Exit runtime and release whatever components we touched
        drop(runtime_guard);

        let measured_execution_time = start.elapsed().as_secs_f32();

        // Make sure unusual spikes don't freeze the emulator (like suspension)
        let is_outlier = if !simulation_controller_state_guard
            .execution_time_ring
            .is_empty()
        {
            let average = simulation_controller_state_guard
                .execution_time_ring
                .iter()
                .copied()
                .sum::<f32>()
                / simulation_controller_state_guard.execution_time_ring.len() as f32;

            measured_execution_time > average * OUTLIER_MULTIPLE
        } else {
            false
        };

        if is_outlier {
            // Edge the execution time upward a bit in case this is a sustained spike in latency
            simulation_controller_state_guard.execution_timeslice +=
                (simulation_controller_state_guard.execution_timeslice / GROWTH_DIVISOR)
                    .max(f32::EPSILON);

            // Assume our old measured timings and calculated error are garbage
            simulation_controller_state_guard
                .execution_time_ring
                .clear();
            simulation_controller_state_guard.error = 0.0;
        } else {
            // Add to ring and compute average value
            simulation_controller_state_guard
                .execution_time_ring
                .enqueue(measured_execution_time);

            let average_execution_time = {
                let sum = simulation_controller_state_guard
                    .execution_time_ring
                    .iter()
                    .copied()
                    .sum::<f32>();

                sum / (simulation_controller_state_guard.execution_time_ring.len() as f32)
            };

            let execution_delta =
                measured_execution_time - simulation_controller_state_guard.execution_timeslice;
            simulation_controller_state_guard.error += execution_delta;

            // Add to ring and compute average value
            let execution_jitter = (measured_execution_time - average_execution_time).abs();
            simulation_controller_state_guard
                .jitter_ring
                .enqueue(execution_jitter);

            let average_execution_jitter = {
                let sum = simulation_controller_state_guard
                    .jitter_ring
                    .iter()
                    .copied()
                    .sum::<f32>();

                sum / (simulation_controller_state_guard.jitter_ring.len() as f32)
            };

            simulation_controller_state_guard.jitter_ratio = (average_execution_jitter
                / simulation_controller_state_guard.execution_timeslice)
                .clamp(0.0, 1.0);

            let stability = 1.0 - simulation_controller_state_guard.jitter_ratio;

            // Sleep correction
            if simulation_controller_state_guard.error
                < -simulation_controller_state_guard.sleep_threshold
            {
                let required_sleep_time = -simulation_controller_state_guard.error;

                let start = Instant::now();
                sleep(Duration::from_secs_f32(required_sleep_time));
                let actual_sleep_time = start.elapsed().as_secs_f32();

                simulation_controller_state_guard.error += actual_sleep_time;

                let sleep_overshoot = actual_sleep_time - required_sleep_time;
                simulation_controller_state_guard
                    .sleep_overshoot_ring
                    .enqueue(sleep_overshoot.max(0.0));
            }

            let average_sleep_overshoot = if simulation_controller_state_guard
                .sleep_overshoot_ring
                .is_empty()
            {
                f32::EPSILON
            } else {
                let sum = simulation_controller_state_guard
                    .sleep_overshoot_ring
                    .iter()
                    .copied()
                    .sum::<f32>();

                sum / (simulation_controller_state_guard.sleep_overshoot_ring.len() as f32)
            };
            simulation_controller_state_guard.sleep_threshold = average_sleep_overshoot * 2.0;

            // Be more aggressive about growing rather than shrinking execution time
            //
            // As its worse to be behind than lock components more than we technically have to for efficiency
            let growth_step = (simulation_controller_state_guard.execution_timeslice
                / GROWTH_DIVISOR)
                * stability.max(0.1);

            let shrink_step = (simulation_controller_state_guard.execution_timeslice
                / SHRINK_DIVISOR)
                * stability.max(0.1);

            let required_timeslice = average_execution_time + average_sleep_overshoot;

            if required_timeslice > simulation_controller_state_guard.execution_timeslice {
                simulation_controller_state_guard.execution_timeslice += growth_step;
            } else {
                simulation_controller_state_guard.execution_timeslice =
                    (simulation_controller_state_guard.execution_timeslice - shrink_step)
                        .max(f32::EPSILON);
            }
        }
    }
}

impl Widget for SimulationControllerState {
    fn ui(self, ui: &mut Ui) -> Response {
        TableBuilder::new(ui)
            .column(Column::auto().resizable(true))
            .column(Column::remainder())
            .striped(true)
            .body(|mut body| {
                let average_execution_time = if self.execution_time_ring.is_empty() {
                    None
                } else {
                    let sum = self.execution_time_ring.iter().copied().sum::<f32>();
                    Some(sum / self.execution_time_ring.len() as f32)
                };

                let average_jitter = if self.jitter_ring.is_empty() {
                    None
                } else {
                    let sum = self.jitter_ring.iter().copied().sum::<f32>();
                    Some(sum / self.jitter_ring.len() as f32)
                };

                let average_sleep_overshoot = if self.sleep_overshoot_ring.is_empty() {
                    None
                } else {
                    let sum = self.sleep_overshoot_ring.iter().copied().sum::<f32>();
                    Some(sum / self.sleep_overshoot_ring.len() as f32)
                };

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
                    t!("simulation_controller.sleep_threshold"),
                    format!("{:?}", Duration::from_secs_f32(self.sleep_threshold)),
                );
                stat_row(
                    t!("simulation_controller.error"),
                    format!("{:?}", Duration::from_secs_f32(self.error.abs())),
                );
                stat_row(
                    t!("simulation_controller.jitter_ratio"),
                    format!("{:.1}%", self.jitter_ratio * 100.0),
                );

                if let Some(average_execution_time) = average_execution_time {
                    stat_row(
                        t!("simulation_controller.average_execution_time"),
                        format!("{:?}", Duration::from_secs_f32(average_execution_time)),
                    );
                }

                if let Some(average_sleep_overshoot) = average_sleep_overshoot {
                    stat_row(
                        t!("simulation_controller.average_sleep_overshoot"),
                        format!("{:?}", Duration::from_secs_f32(average_sleep_overshoot)),
                    );
                }

                if let Some(average_jitter) = average_jitter {
                    stat_row(
                        t!("simulation_controller.average_jitter"),
                        format!("{:?}", Duration::from_secs_f32(average_jitter)),
                    );
                }
            });

        ui.allocate_rect(ui.min_rect(), Sense::empty())
    }
}
