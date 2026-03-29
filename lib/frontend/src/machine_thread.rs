use std::{
    sync::{
        Arc,
        mpsc::{self, TryRecvError},
    },
    thread::sleep,
    time::Instant,
};

use fluxemu_runtime::machine::Machine;
use ringbuffer::{ConstGenericRingBuffer, RingBuffer};
use time::Duration;

const ALPHA: f32 = 0.1;
const SMOOTHING_WINDOW: usize = 4;

#[derive(Debug, Clone)]
pub enum Message {
    Pause(bool),
}

pub struct MachineThreadContext {
    pub message_receiver: mpsc::Receiver<Message>,
    pub machine: Arc<Machine>,
}

pub fn machine_thread(
    MachineThreadContext {
        message_receiver,
        machine,
    }: MachineThreadContext,
) {
    let min_timeslice = Duration::milliseconds(4);
    let mut execution_timeslice = min_timeslice;
    let mut sleep_threshold = Duration::milliseconds(1);
    let mut error = Duration::ZERO;
    let mut average_sleep_overshoot = Duration::ZERO;
    let mut paused = false;

    // Ring buffers for smoothing
    let mut execution_time_buffer = ConstGenericRingBuffer::<Duration, SMOOTHING_WINDOW>::new();
    let mut jitter_buffer = ConstGenericRingBuffer::<Duration, SMOOTHING_WINDOW>::new();

    loop {
        if paused {
            match message_receiver.recv() {
                Ok(Message::Pause(state)) => paused = state,
                Err(_) => return,
            }
            continue;
        }

        let start = Instant::now();

        machine
            .enter_runtime()
            .run_duration(execution_timeslice.try_into().unwrap());

        let execution_time: Duration = start.elapsed().try_into().unwrap();

        // Add to buffer and compute smoothed value
        execution_time_buffer.enqueue(execution_time);
        let smoothed_exec_time = {
            let sum: Duration = execution_time_buffer.iter().copied().sum();
            sum / (execution_time_buffer.len() as i32)
        };

        let frame_delta = smoothed_exec_time - execution_timeslice;

        // Jitter smoothing
        jitter_buffer.enqueue(frame_delta.abs());
        let average_jitter: Duration = {
            let sum: Duration = jitter_buffer.iter().copied().sum();
            sum / (jitter_buffer.len() as i32)
        };
        let jitter_ratio = if execution_timeslice > Duration::ZERO {
            (average_jitter.as_seconds_f32() / execution_timeslice.as_seconds_f32()).clamp(0.0, 1.0)
        } else {
            0.0
        };

        let stability = 1.0 - jitter_ratio;
        error += smoothed_exec_time - execution_timeslice;

        // Avoid windup
        let max_error = execution_timeslice * 4;
        if error > max_error {
            error = max_error;
        } else if error < -max_error {
            error = -max_error;
        }

        // Sleep correction
        if error < -sleep_threshold {
            let requested = -error;
            let sleep_start = Instant::now();
            sleep(requested.try_into().unwrap());
            let actual_sleep_time = sleep_start.elapsed();

            error += actual_sleep_time;

            let overshoot = actual_sleep_time - requested;
            average_sleep_overshoot =
                average_sleep_overshoot + (overshoot - average_sleep_overshoot) * ALPHA;

            sleep_threshold = average_sleep_overshoot.max(Duration::ZERO) * 2;
        }

        let growth_step: Duration = execution_timeslice / 16;
        let shrink_step: Duration = execution_timeslice / 8;

        let growth_step = growth_step * stability.max(0.1);
        let shrink_step = shrink_step * stability.max(0.1);

        let required_timeslice = smoothed_exec_time + average_sleep_overshoot;

        if required_timeslice > execution_timeslice {
            execution_timeslice += growth_step;
        } else if execution_timeslice > min_timeslice + shrink_step {
            execution_timeslice = (execution_timeslice - shrink_step).max(min_timeslice);
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
