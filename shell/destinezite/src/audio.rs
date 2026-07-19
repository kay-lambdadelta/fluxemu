use std::{
    fmt::Debug,
    sync::{Arc, OnceLock},
};

use bytemuck::Pod;
use cpal::{
    Device, Stream, SupportedStreamConfig,
    traits::{DeviceTrait, HostTrait, StreamTrait},
};
use fluxemu_audio::{FromSample, SampleFormat};
use fluxemu_frontend::audio::{AudioRuntime, mixer::AudioMixer};
use nalgebra::SVector;

pub struct CpalAudioRuntime {
    stream: Stream,
    config: SupportedStreamConfig,
    mixer: Arc<OnceLock<Arc<AudioMixer>>>,
}

impl Debug for CpalAudioRuntime {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CpalAudioRuntime").finish()
    }
}

impl CpalAudioRuntime {
    pub fn new() -> Result<Self, ()> {
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

        let default_output_config = device.default_output_config().unwrap();
        let mut supported_output_configs = device.supported_output_configs().unwrap();

        let config_range = supported_output_configs
            .find(|config| {
                config.channels() == 2
                    && config.sample_format() == cpal::SampleFormat::F32
                    && (config.min_sample_rate()..=config.max_sample_rate())
                        .contains(&default_output_config.sample_rate())
            })
            .unwrap();

        let config = config_range.with_sample_rate(default_output_config.sample_rate());
        tracing::info!("Selected audio device with config: {:#?}", config);

        let mixer: Arc<OnceLock<_>> = Arc::default();

        let stream = create_stream::<f32, 2>(&device, config, mixer.clone());

        Ok(Self {
            stream,
            config,
            mixer,
        })
    }
}

impl AudioRuntime for CpalAudioRuntime {
    fn sample_rate(&mut self) -> f32 {
        self.config.sample_rate() as f32
    }

    fn pause(&mut self) {
        self.stream.pause().unwrap();
    }

    fn play(&mut self) {
        self.stream.play().unwrap();
    }

    fn set_audio_mixer(&mut self, audio_mixer: Arc<AudioMixer>) {
        self.mixer.set(audio_mixer).unwrap();
    }
}

fn create_stream<
    S: SampleFormat + FromSample<f32> + cpal::SizedSample + Pod,
    const CHANNELS: usize,
>(
    device: &Device,
    config: cpal::SupportedStreamConfig,
    mixer: Arc<OnceLock<Arc<AudioMixer>>>,
) -> Stream {
    device
        .build_output_stream::<S, _, _>(
            config.config(),
            move |buffer, _info| {
                if let Some(mixer) = mixer.get() {
                    let buffer: &mut [SVector<S, CHANNELS>] = bytemuck::cast_slice_mut(buffer);

                    mixer.write_buffer(buffer);
                } else {
                    buffer.fill(S::equilibrium());
                }
            },
            |e| {
                tracing::error!("{}", e);
            },
            None,
        )
        .unwrap()
}
