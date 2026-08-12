use std::sync::Arc;

#[derive(Debug)]
pub struct AudioRuntime;

impl fluxemu_frontend::audio::AudioRuntime for AudioRuntime {
    fn sample_rate(&mut self) -> f32 {
        todo!()
    }

    fn set_audio_mixer(&mut self, _audio_mixer: Arc<fluxemu_frontend::audio::mixer::AudioMixer>) {
        todo!()
    }
}
