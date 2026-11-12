use std::sync::Arc;

use fluxemu_frontend::audio::mixer::AudioMixer;

#[derive(Debug)]
pub struct AudioRuntime;

impl fluxemu_frontend::audio::AudioRuntime for AudioRuntime {
    fn sample_rate(&mut self) -> f32 {
        48000.0
    }

    fn set_audio_mixer(&mut self, _audio_mixer: Arc<AudioMixer>) {}
}
