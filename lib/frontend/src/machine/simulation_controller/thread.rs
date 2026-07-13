use std::{
    fmt::Debug,
    sync::{Arc, atomic::Ordering},
    time::{Duration, Instant},
};

use fluxemu_runtime::machine::Machine;
use rand::RngExt;
use ringbuffer::{ConstGenericRingBuffer, RingBuffer};
use rust_i18n::t;

use crate::{
    audio::mixer::AudioMixer,
    machine::simulation_controller::{
        COMFORTABLE_HEADROOM, DIMINISHING_RETURNS_ELASTICITY, EXPLORATION_CHANGE,
        HARDWARE_SPEED_EMA, HISTORICAL_SAMPLE_WINDOW, JITTER_CEILING, MAX_SCHEDULE_DRIFT,
        MIN_PROBE_DELTA, OVERSHOOT_EMA_ALPHA, PROBE_WINDOW, SharedState,
    },
};

#[derive(Clone, Debug)]
pub struct SimulationControllerState {
    /// The current time allocated per loop to the guest machine scheduler
    pub execution_timeslice: f32,
    /// The value the execution timeslice is currently moving toward
    pub target_timeslice: f32,
    pub jitter_ratio: f32,
    /// How fast we are running on the host hardware
    pub hardware_speed_ema: f32,
    pub last_probe_timeslice: f32,
    pub last_probe_hardware_speed: f32,
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
            last_probe_hardware_speed: 1.0,
            last_probe_timeslice: 1.0,
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
        if shared.should_exit.load(Ordering::Acquire) {
            break;
        }

        if shared.paused.load(Ordering::Acquire) {
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

        // We want to try to execute slightly faster than realtime, to compensate for overhead/jitter we don't measure
        let target_iteration_time = state.target_timeslice / COMFORTABLE_HEADROOM;

        let now = Instant::now();
        let deadline = next_deadline.unwrap_or(now);
        let deadline = if now.saturating_duration_since(deadline) > MAX_SCHEDULE_DRIFT {
            now
        } else {
            deadline
        };
        let deadline = deadline + Duration::from_secs_f32(target_iteration_time);
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

        let hardware_speed = if measured_execution_time > 0.0 {
            execution_timeslice / measured_execution_time
        } else {
            1.0
        };

        state.hardware_speed_ema = HARDWARE_SPEED_EMA * state.hardware_speed_ema
            + (1.0 - HARDWARE_SPEED_EMA) * hardware_speed;

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

            let relative_move = (state.execution_timeslice - state.last_probe_timeslice).abs()
                / state.last_probe_timeslice;

            let step = EXPLORATION_CHANGE.as_secs_f32();

            let walk = if relative_move >= MIN_PROBE_DELTA {
                let current_speed = ring_median(&state.probe_speed_ring);
                let delta_speed = current_speed - state.last_probe_hardware_speed;
                let elasticity = (delta_speed / state.last_probe_hardware_speed) / relative_move;

                state.last_probe_timeslice = state.execution_timeslice;
                state.last_probe_hardware_speed = current_speed;

                if elasticity > DIMINISHING_RETURNS_ELASTICITY {
                    rand::rng().random_range(-(step * 0.2)..=step)
                } else {
                    rand::rng().random_range(-step..=(step * 0.2))
                }
            } else {
                // Move randomly until we have more coherent data
                rand::rng().random_range(-step..=(step * 0.99))
            };

            target += walk;
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
