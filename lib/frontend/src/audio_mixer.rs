use std::{collections::HashMap, sync::Mutex};

use fluxemu_audio::{FrameIterator, FromSample, Nearest, SampleFormat};
use fluxemu_runtime::{ResourcePath, machine::RuntimeGuard};
use nalgebra::SVector;
use ringbuffer::{AllocRingBuffer, RingBuffer};

#[derive(Debug)]
struct State {
    audio_ring: AllocRingBuffer<SVector<f32, 2>>,
    interpolaters: HashMap<ResourcePath, Nearest<f32, 1>>,
}

#[derive(Debug)]
pub struct AudioMixer {
    output_sample_rate: f32,
    state: Mutex<State>,
}

impl AudioMixer {
    pub fn new(sample_rate: f32) -> Self {
        Self {
            output_sample_rate: sample_rate,
            state: Mutex::new(State {
                audio_ring: AllocRingBuffer::new(sample_rate as usize * 10),
                interpolaters: HashMap::default(),
            }),
        }
    }

    pub fn extract_machine_samples(&self, runtime_guard: &RuntimeGuard<'_>) {
        let mut state_guard = self.state.lock().unwrap();
        let state_guard = &mut *state_guard;

        for audio_stream_path in runtime_guard.audio_outputs() {
            let Some(component_path) = audio_stream_path.parent() else {
                continue;
            };

            runtime_guard
                .registry()
                .interact_dyn(
                    component_path,
                    runtime_guard.safe_advance_timestamp(),
                    |component| {
                        let source = component.get_audio_channel(audio_stream_path.name());

                        let interpolater = state_guard
                            .interpolaters
                            .entry(audio_stream_path.clone())
                            .or_insert_with(|| {
                                Nearest::new(source.sample_rate, self.output_sample_rate)
                            });

                        // Audio output changed its sample rate, reset the interpolater
                        if source.sample_rate != interpolater.source_rate() {
                            *interpolater =
                                Nearest::new(source.sample_rate, self.output_sample_rate);
                        }

                        let frames = source
                            .audio_ring
                            .drain()
                            .resample(interpolater)
                            .remix::<2>();

                        state_guard.audio_ring.extend(frames);
                    },
                )
                .unwrap();
        }
    }

    pub fn write_buffer<S: SampleFormat + FromSample<f32>, const CHANNELS: usize>(
        &self,
        buffer: &mut [SVector<S, CHANNELS>],
    ) {
        buffer.fill(SVector::from_element(S::equilibrium()));

        let mut state_guard = self.state.lock().unwrap();

        for (src, dst) in state_guard
            .audio_ring
            .drain()
            .pad()
            .rescale::<S>()
            .remix()
            .zip(buffer.iter_mut())
        {
            *dst = src;
        }
    }
}
