use std::{
    fmt::Debug,
    sync::{Arc, atomic::Ordering},
    time::{Duration, Instant},
};

use fluxemu_runtime::machine::Machine;
use rand::RngExt;
use ringbuffer::{ConstGenericRingBuffer, RingBuffer};
use rust_i18n::t;

pub use super::{
    DIMINISHING_RETURNS_ELASTICITY, EXPLORATION_CHANGE, HARDWARE_SPEED_TIME_CONSTANT,
    HISTORICAL_SAMPLE_WINDOW, JITTER_CEILING, MAX_SCHEDULE_DRIFT, MIN_PROBE_DELTA,
    OVERSHOOT_EMA_ALPHA, PROBE_INTERVAL, PROBE_WINDOW,
};
use crate::{audio::mixer::AudioMixer, simulation_controller::SharedState};

#[derive(Clone, Copy, Debug, Default)]
pub enum WalkBias {
    #[default]
    Neutral,
    Grow,
    Shrink,
}

impl WalkBias {
    fn sample(self, step: f32) -> f32 {
        match self {
            // Move randomly until we have more coherent data
            Self::Neutral => rand::rng().random_range(-step..=(step * 0.99)),
            Self::Grow => rand::rng().random_range(-(step * 0.2)..=step),
            Self::Shrink => rand::rng().random_range(-step..=(step * 0.2)),
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Probe {
    pub timeslice: f32,
    pub hardware_speed: f32,
}

#[derive(Clone, Copy, Debug)]
pub struct ExecutionAverages {
    pub guest_time: f32,
    pub host_time: f32,
}

#[derive(Clone, Debug)]
pub struct SimulationControllerState {
    /// The current time allocated per loop to the guest machine scheduler
    pub execution_timeslice: f32,
    /// The value the execution timeslice is currently moving toward
    pub target_timeslice: f32,
    pub jitter_ratio: f32,
    /// How fast we are running on the host hardware
    pub hardware_speed_ema: f32,
    pub execution_averages: Option<ExecutionAverages>,
    pub last_probe: Option<Probe>,
    pub time_since_probe: f32,
    pub walk_bias: WalkBias,
    pub probe_speed_ring: ConstGenericRingBuffer<f32, PROBE_WINDOW>,
    pub execution_time_ring: ConstGenericRingBuffer<f32, HISTORICAL_SAMPLE_WINDOW>,
    pub execution_jitter_ring: ConstGenericRingBuffer<f32, HISTORICAL_SAMPLE_WINDOW>,
}

impl Default for SimulationControllerState {
    fn default() -> Self {
        let initial = Duration::from_millis(1).as_secs_f32();

        Self {
            execution_timeslice: initial,
            target_timeslice: initial,
            execution_averages: None,
            last_probe: None,
            time_since_probe: 0.0,
            walk_bias: WalkBias::default(),
            hardware_speed_ema: 1.0,
            jitter_ratio: 0.0,
            execution_time_ring: ConstGenericRingBuffer::default(),
            execution_jitter_ring: ConstGenericRingBuffer::default(),
            probe_speed_ring: ConstGenericRingBuffer::default(),
        }
    }
}

pub fn simulation_controller_loop(
    machine: Arc<Machine>,
    audio_mixer: Arc<AudioMixer>,
    shared: Arc<SharedState>,
) {
    let mut next_deadline = None;
    let mut sleep_overshoot_ema = 0.0;

    loop {
        if shared.should_exit.load(Ordering::Relaxed) {
            break;
        }

        if shared.paused.load(Ordering::Relaxed) {
            std::thread::park();
            next_deadline = None;
            continue;
        }

        let execution_timeslice = {
            let guard = shared.state.lock().unwrap();
            guard.execution_timeslice
        };

        let start = Instant::now();
        let measured_execution_time = {
            let runtime_guard = machine.enter_runtime();
            runtime_guard.run_duration(Duration::from_secs_f32(execution_timeslice));
            audio_mixer.extract_machine_samples(&runtime_guard);
            start.elapsed().as_secs_f32()
        };

        if !measured_execution_time.is_finite() || measured_execution_time <= 0.0 {
            tracing::warn!("{}", t!("simulation_controller.invalid_measurement"));

            // The OS timer might be busted, loop and try again
            continue;
        }

        let mut state = shared.state.lock().unwrap();

        let now = Instant::now();
        let deadline = next_deadline.unwrap_or(now);
        let deadline = if now.saturating_duration_since(deadline) > MAX_SCHEDULE_DRIFT {
            now
        } else {
            deadline
        };
        let deadline = deadline + Duration::from_secs_f32(execution_timeslice);
        next_deadline = Some(deadline);

        let raw_sleep_time = deadline.saturating_duration_since(now).as_secs_f32();
        let biased_sleep_time = (raw_sleep_time - sleep_overshoot_ema).max(0.0);

        if biased_sleep_time > 0.0 {
            drop(state);

            let sleep_start = Instant::now();
            std::thread::sleep(Duration::from_secs_f32(biased_sleep_time));
            let actual_slept = sleep_start.elapsed().as_secs_f32();

            state = shared.state.lock().unwrap();

            // React slowly to the overshoot, do not overreact to hiccups
            let overshoot = actual_slept - biased_sleep_time;
            sleep_overshoot_ema =
                OVERSHOOT_EMA_ALPHA * sleep_overshoot_ema + (1.0 - OVERSHOOT_EMA_ALPHA) * overshoot;
        }

        let total_iteration_time = start.elapsed().as_secs_f32();

        let median_iteration_time = ring_median(&state.execution_time_ring);
        let iteration_jitter = (total_iteration_time - median_iteration_time).abs();
        let median_jitter = ring_median(&state.execution_jitter_ring);

        state.execution_jitter_ring.enqueue(iteration_jitter);
        state.execution_time_ring.enqueue(total_iteration_time);

        let retained = (-total_iteration_time / HARDWARE_SPEED_TIME_CONSTANT.as_secs_f32()).exp();
        let averages = state.execution_averages.map_or(
            ExecutionAverages {
                guest_time: execution_timeslice,
                host_time: measured_execution_time,
            },
            |averages| ExecutionAverages {
                guest_time: retained * averages.guest_time + (1.0 - retained) * execution_timeslice,
                host_time: retained * averages.host_time
                    + (1.0 - retained) * measured_execution_time,
            },
        );

        state.execution_averages = Some(averages);
        state.hardware_speed_ema = averages.guest_time / averages.host_time;
        state.time_since_probe += total_iteration_time;

        state.jitter_ratio = if state.target_timeslice > 0.0 {
            (median_jitter / state.target_timeslice).clamp(0.0, 1.0)
        } else {
            0.0
        };

        let mut target = state.target_timeslice;

        let jitter_unsafe = state.jitter_ratio >= JITTER_CEILING;

        if jitter_unsafe {
            // Upward pressure on the execution timeslice to escape severe OS/scheduler jitter
            //
            // This effectively guards it against being too small/dealing with a severely overloaded os scheduler
            target += EXPLORATION_CHANGE.as_secs_f32();
        } else {
            // We only take speed results into the ring when we can confirm we are stable enough for the value to make any sense
            let hardware_speed_ema = state.hardware_speed_ema;
            state.probe_speed_ring.enqueue(hardware_speed_ema);

            if state.time_since_probe >= PROBE_INTERVAL.as_secs_f32() {
                let current = Probe {
                    timeslice: averages.guest_time,
                    hardware_speed: ring_median(&state.probe_speed_ring),
                };

                match state.last_probe {
                    Some(last_probe) => {
                        let relative_move =
                            (current.timeslice - last_probe.timeslice) / last_probe.timeslice;

                        if relative_move.abs() >= MIN_PROBE_DELTA {
                            let delta_speed = current.hardware_speed - last_probe.hardware_speed;
                            let elasticity =
                                (delta_speed / last_probe.hardware_speed) / relative_move;

                            state.walk_bias = if elasticity > DIMINISHING_RETURNS_ELASTICITY {
                                WalkBias::Grow
                            } else {
                                WalkBias::Shrink
                            };
                            state.last_probe = Some(current);
                            state.time_since_probe = 0.0;
                        }
                    }
                    None => {
                        state.last_probe = Some(current);
                        state.time_since_probe = 0.0;
                    }
                }
            }

            target += state.walk_bias.sample(EXPLORATION_CHANGE.as_secs_f32());
        }

        if target.is_finite() && target > f32::EPSILON {
            state.target_timeslice = target;

            // Put the execution timeslice on a rubber band so visible jumps in emulation speed don't occur
            let max_step = Duration::from_micros(1).as_secs_f32();
            let difference = state.target_timeslice - state.execution_timeslice;
            state.execution_timeslice += difference.clamp(-max_step, max_step);
        }
    }
}

// We take the median and not the mean because its more robust to outliers
#[inline]
fn ring_median<const N: usize>(ring: &ConstGenericRingBuffer<f32, N>) -> f32 {
    if ring.is_empty() {
        return 0.0;
    }

    let mut values = [0.0; N];
    let len = ring.len();
    for (slot, value) in values.iter_mut().zip(ring.iter()) {
        *slot = *value;
    }

    values[..len].sort_unstable_by(|a, b| a.partial_cmp(b).unwrap());
    values[len / 2]
}
