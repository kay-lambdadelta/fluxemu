use std::{fmt::Debug, sync::Arc};

use arc_swap::ArcSwapOption;
use cpal::{
    Device, Host, Stream,
    traits::{DeviceTrait, HostTrait, StreamTrait},
};
use fluxemu_audio::{Cubic, FrameIterator, Linear, SampleFormat};
use fluxemu_frontend::AudioRuntime;
use fluxemu_runtime::{machine::Machine, platform::Platform, scheduler::Period};
use itertools::Itertools;
use nalgebra::SVector;
use ringbuffer::RingBuffer;

pub struct CpalAudioRuntime {
    #[allow(unused)]
    host: Host,
    #[allow(unused)]
    device: Device,
    stream: Stream,
    machine: Arc<ArcSwapOption<Machine>>,
}

impl Debug for CpalAudioRuntime {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CpalAudioRuntime").finish()
    }
}

impl<P: Platform> AudioRuntime<P> for CpalAudioRuntime {
    fn new() -> Result<Self, Box<dyn std::error::Error>> {
        let host = cpal::default_host();
        tracing::info!("Selecting audio api {:?}", host.id());

        let device = host
            .default_output_device()
            .expect("failed to get default output device");

        if let Ok(description) = device.description() {
            tracing::info!("Selected audio device with properties {:?}", description);
        } else {
            tracing::info!("Selected audio device");
        }

        let sample_rate = device.default_output_config().unwrap().sample_rate();
        let config = device
            .supported_output_configs()
            .unwrap()
            .sorted_by_key(|config| config.sample_format() == cpal::SampleFormat::F32)
            .rev()
            .find(|config| config.channels() == 2)
            .unwrap()
            .with_sample_rate(sample_rate);

        tracing::info!("Selected audio device with config: {:#?}", config);

        let sample_rate = sample_rate as f32;

        let machine: Arc<ArcSwapOption<Machine>> = Arc::default();

        let stream = device
            .build_output_stream::<f32, _, _>(
                &config.config(),
                {
                    let machine = machine.clone();

                    move |buffer, _info| {
                        let machine = machine.load();

                        if let Some(machine) = machine.as_ref() {
                            let buffer: &mut [SVector<f32, _>] = bytemuck::cast_slice_mut(buffer);
                            buffer.fill(SVector::from_element(f32::equilibrium()));

                            let representing_time =
                                Period::from_num(buffer.len() as f32 / sample_rate);

                            for audio_stream in &machine.audio_outputs {
                                machine
                                    .interact_dyn_mut(audio_stream, |component| {
                                        let audio_source =
                                            component.get_audio_channel(audio_stream);

                                        let audio_generator = audio_source
                                            .source
                                            .drain()
                                            .pad()
                                            .resample::<f32>(
                                                audio_source.sample_rate,
                                                sample_rate,
                                                Linear,
                                            )
                                            .remix::<2>();

                                        for (destination, source) in
                                            buffer.iter_mut().zip(audio_generator)
                                        {
                                            *destination = source;
                                        }
                                    })
                                    .unwrap()
                            }
                        }
                    }
                },
                |e| {
                    tracing::error!("{}", e);
                },
                None,
            )
            .unwrap();

        Ok(Self {
            host,
            device,
            stream,
            machine,
        })
    }

    fn pause(&mut self) {
        self.stream.pause().unwrap();
    }

    fn play(&mut self) {
        self.stream.play().unwrap();
    }

    fn set_machine(&mut self, machine: Option<Arc<Machine>>) {
        self.machine.store(machine);
    }
}
