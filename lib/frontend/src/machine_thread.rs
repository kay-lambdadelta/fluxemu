use std::{
    sync::{
        Arc,
        mpsc::{self, TryRecvError},
    },
    thread::sleep,
    time::{Duration, Instant},
};

use fluxemu_runtime::machine::Machine;
use ringbuffer::{ConstGenericRingBuffer, RingBuffer};

use crate::audio_mixer::AudioMixer;

const ALPHA: f32 = 0.1;
const SMOOTHING_WINDOW: usize = 8;
const OUTLIER_MULTIPLE: f32 = 10.0;
const GROWTH_DIVISOR: f32 = 8.0;
const SHRINK_DIVISOR: f32 = 16.0;
const TRACE_INTERVAL: Duration = Duration::from_secs(10);

#[derive(Debug, Clone)]
pub enum Message {
    Pause(bool),
}

pub struct MachineThreadContext {
    pub message_receiver: mpsc::Receiver<Message>,
    pub machine: Arc<Machine>,
    pub audio_mixer: Arc<AudioMixer>,
}

pub fn machine_thread(
    MachineThreadContext {
        message_receiver,
        machine,
        audio_mixer,
    }: MachineThreadContext,
) {
    let initial_time = Duration::from_millis(4).as_secs_f32();
    let mut execution_timeslice = initial_time;
    let mut sleep_threshold = initial_time;
    let mut error = 0.0;
    let mut average_sleep_overshoot = 0.0;
    let mut paused = false;
    let mut last_trace = Instant::now();

    // Ring buffers for smoothing
    let mut execution_time_buffer = ConstGenericRingBuffer::<f32, SMOOTHING_WINDOW>::new();
    let mut jitter_buffer = ConstGenericRingBuffer::<f32, SMOOTHING_WINDOW>::new();

    loop {
        if paused {
            match message_receiver.recv() {
                Ok(Message::Pause(state)) => paused = state,
                Err(_) => return,
            }
            continue;
        }

        let start = Instant::now();

        // Enter runtime
        let runtime_guard = machine.enter_runtime();
        // Run simulation
        runtime_guard.run_duration(Duration::from_secs_f32(execution_timeslice));
        // Extract audio samples
        audio_mixer.extract_machine_samples(&runtime_guard);
        // Exit runtime and release whatever components we touched
        drop(runtime_guard);

        let execution_time = start.elapsed().as_secs_f32();

        // Make sure unusual spikes don't freeze the emulator (like suspension)
        let is_outlier = if execution_time_buffer.len() > 1 {
            let average = execution_time_buffer.iter().copied().sum::<f32>()
                / execution_time_buffer.len() as f32;

            execution_time > average * OUTLIER_MULTIPLE
        } else {
            false
        };

        if is_outlier {
            error = 0.0;

            execution_timeslice += (execution_timeslice / GROWTH_DIVISOR).max(f32::EPSILON);
            execution_time_buffer.clear();
        } else {
            // Add to buffer and compute smoothed value
            execution_time_buffer.enqueue(execution_time);
            let smoothed_exec_time = {
                let sum = execution_time_buffer.iter().copied().sum::<f32>();

                sum / (execution_time_buffer.len() as f32)
            };

            let frame_delta = smoothed_exec_time - execution_timeslice;

            // Jitter smoothing
            jitter_buffer.enqueue(frame_delta.abs());
            let average_jitter = {
                let sum = jitter_buffer.iter().copied().sum::<f32>();

                sum / (jitter_buffer.len() as f32)
            };
            let jitter_ratio = if execution_timeslice > 0.0 {
                (average_jitter / execution_timeslice).clamp(0.0, 1.0)
            } else {
                0.0
            };

            let stability = 1.0 - jitter_ratio;
            error += smoothed_exec_time - execution_timeslice;

            // Avoid windup
            let max_error = execution_timeslice * 4.0;
            if error > max_error {
                error = max_error;
            } else if error < -max_error {
                error = -max_error;
            }

            // Sleep correction
            if error < -sleep_threshold {
                let requested = -error;
                let sleep_start = Instant::now();
                sleep(Duration::from_secs_f32(requested));
                let actual_sleep_time = sleep_start.elapsed().as_secs_f32();

                error += actual_sleep_time;

                let overshoot = actual_sleep_time - requested;
                average_sleep_overshoot =
                    average_sleep_overshoot + (overshoot - average_sleep_overshoot) * ALPHA;

                sleep_threshold = average_sleep_overshoot.max(0.0) * 2.0;
            }

            // Be more aggressive about growing rather than shrinking execution time
            //
            // As its worse to be behind than lock components more than we technically have to for efficiency

            let growth_step = (execution_timeslice / GROWTH_DIVISOR) * stability.max(0.1);
            let shrink_step = (execution_timeslice / SHRINK_DIVISOR) * stability.max(0.1);

            let required_timeslice = smoothed_exec_time + average_sleep_overshoot;

            if required_timeslice > execution_timeslice {
                execution_timeslice += growth_step;
            } else {
                execution_timeslice = (execution_timeslice - shrink_step).max(f32::EPSILON);
            }

            if last_trace.elapsed() >= TRACE_INTERVAL {
                // Helpful debug logs

                tracing::debug!(
                    execution_timeslice = ?Duration::from_secs_f32(execution_timeslice),
                    smoothed_exec = ?Duration::from_secs_f32(smoothed_exec_time),
                    actual_exec = ?Duration::from_secs_f32(execution_time),
                    error = ?Duration::from_secs_f32(error.abs()),
                    sleep_threshold = ?Duration::from_secs_f32(sleep_threshold),
                    average_sleep_overshoot = ?Duration::from_secs_f32(average_sleep_overshoot.abs()),
                    jitter_ratio,
                    stability,
                    "controller state"
                );

                last_trace = Instant::now();
            }
        }

        loop {
            match message_receiver.try_recv() {
                Ok(message) => match message {
                    Message::Pause(state) => paused = state,
                },
                Err(TryRecvError::Disconnected) => {
                    return;
                }
                Err(TryRecvError::Empty) => {
                    break;
                }
            }
        }
    }
}
