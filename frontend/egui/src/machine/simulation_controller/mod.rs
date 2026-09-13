use std::{
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    thread::JoinHandle,
    time::Duration,
};

use fluxemu_runtime::machine::Machine;

use crate::{
    AudioMixer,
    machine::simulation_controller::{
        thread::{SimulationControllerState, simulation_controller_loop},
        ui::UiState,
    },
};

const UI_UPDATE_RATE: Duration = Duration::from_millis(200);
const HISTORICAL_SAMPLE_WINDOW: usize = 32;
const JITTER_CEILING: f32 = 0.4;
const HARDWARE_SPEED_EMA: f32 = 0.9995;
const COMFORTABLE_HEADROOM: f32 = 1.0 + Duration::from_millis(1).as_secs_f32();
const EXPLORATION_CHANGE: Duration = Duration::from_micros(1);
const MAX_SCHEDULE_DRIFT: Duration = Duration::from_millis(20);
const OVERSHOOT_EMA_ALPHA: f32 = 0.9;
const DIMINISHING_RETURNS_ELASTICITY: f32 = 0.4;
const MIN_PROBE_DELTA: f32 = 0.05;
const PROBE_WINDOW: usize = 64;

mod thread;
mod ui;

#[derive(Debug)]
pub struct SimulationController {
    shared: Arc<SharedState>,
    ui_state: UiState,
    handle: Option<JoinHandle<()>>,
}

impl Drop for SimulationController {
    fn drop(&mut self) {
        self.shared.should_exit.store(true, Ordering::Release);
        self.shared.paused.store(false, Ordering::Release);

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
            state: Mutex::default(),
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
            ui_state: UiState::default(),
        }
    }

    pub fn set_paused(&self, paused: bool) {
        self.shared.paused.store(paused, Ordering::Release);

        self.handle.as_ref().unwrap().thread().unpark();
    }
}

#[derive(Debug)]
struct SharedState {
    paused: AtomicBool,
    should_exit: AtomicBool,
    state: Mutex<SimulationControllerState>,
}
