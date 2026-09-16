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
    audio::mixer::AudioMixer,
    simulation_controller::thread::{SimulationControllerState, simulation_controller_loop},
};

mod thread;

pub const HISTORICAL_SAMPLE_WINDOW: usize = 32;
pub const JITTER_CEILING: f32 = 0.4;
pub const HARDWARE_SPEED_EMA: f32 = 0.9995;
pub const COMFORTABLE_HEADROOM: f32 = 1.0 + Duration::from_millis(1).as_secs_f32();
pub const EXPLORATION_CHANGE: Duration = Duration::from_micros(1);
pub const MAX_SCHEDULE_DRIFT: Duration = Duration::from_millis(20);
pub const OVERSHOOT_EMA_ALPHA: f32 = 0.9;
pub const DIMINISHING_RETURNS_ELASTICITY: f32 = 0.4;
pub const MIN_PROBE_DELTA: f32 = 0.05;
pub const PROBE_WINDOW: usize = 64;

#[derive(Debug)]
pub struct Controller {
    shared: Arc<SharedState>,
    handle: Option<JoinHandle<()>>,
}

impl Drop for Controller {
    fn drop(&mut self) {
        self.shared.should_exit.store(true, Ordering::Release);
        self.shared.paused.store(false, Ordering::Release);

        let handle = self.handle.take().unwrap();
        handle.thread().unpark();

        handle.join().unwrap();
    }
}

impl Controller {
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
        }
    }

    pub fn set_paused(&self, paused: bool) {
        self.shared.paused.store(paused, Ordering::Release);

        self.handle.as_ref().unwrap().thread().unpark();
    }

    pub fn get_state_snapshot(&self) -> SimulationControllerState {
        self.shared.state.lock().unwrap().clone()
    }
}

#[derive(Debug)]
struct SharedState {
    paused: AtomicBool,
    should_exit: AtomicBool,
    state: Mutex<SimulationControllerState>,
}
