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
const SMOOTHING_WINDOW: usize = 4;
const OUTLIER_MULTIPLE: f32 = 10.0;
const MIN_TIMESLICE: f32 = Duration::from_millis(4).as_secs_f32();

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
    let mut execution_timeslice = MIN_TIMESLICE;
    let mut sleep_threshold = Duration::from_millis(4).as_secs_f32();
    let mut error = 0.0;
    let mut average_sleep_overshoot = 0.0;
    let mut paused = false;

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

            let growth_step = execution_timeslice / 16.0;
            let shrink_step = execution_timeslice / 8.0;

            let growth_step = growth_step * stability.max(0.1);
            let shrink_step = shrink_step * stability.max(0.1);

            let required_timeslice = smoothed_exec_time + average_sleep_overshoot;

            if required_timeslice > execution_timeslice {
                execution_timeslice += growth_step;
            } else if execution_timeslice > MIN_TIMESLICE + shrink_step {
                execution_timeslice = (execution_timeslice - shrink_step).max(MIN_TIMESLICE);
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
