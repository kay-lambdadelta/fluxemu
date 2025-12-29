use std::{fmt::Debug, sync::Arc};

use arc_swap::ArcSwapOption;
use cpal::{
    Device, Host, Stream,
    traits::{DeviceTrait, HostTrait, StreamTrait},
};
use fluxemu_audio::{Cubic, FrameIterator};
use fluxemu_frontend::AudioRuntime;
use fluxemu_runtime::{machine::Machine, platform::Platform};
use itertools::Itertools;

#[allow(unused)]
pub struct CpalAudioRuntime {
    host: Host,
    device: Device,
    sample_rate: f32,
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

                    move |buffer, info| {
                        let machine = machine.load();

                        if let Some(machine) = machine.as_ref() {
                            for audio_stream in &machine.audio_outputs {
                                machine
                                    .interact_dyn_mut(audio_stream, |component| {
                                        let audio_source =
                                            component.get_audio_channel(audio_stream);

                                        for (source, destination) in audio_source
                                            .source
                                            .resample::<f32>(
                                                audio_source.sample_rate,
                                                config.sample_rate() as f32,
                                                Cubic,
                                            )
                                            .remix::<2>()
                                            .zip(
                                                buffer
                                                    .as_chunks_mut::<2>()
                                                    .0
                                                    .iter_mut()
                                                    .map(bytemuck::cast_mut),
                                            )
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

        stream.play().unwrap();

        Ok(Self {
            host,
            device,
            stream,
            sample_rate,
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
