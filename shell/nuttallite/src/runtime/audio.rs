use std::sync::Arc;

#[derive(Debug)]
pub struct AudioRuntime;

impl fluxemu_frontend_egui::audio::AudioRuntime for AudioRuntime {
    fn sample_rate(&mut self) -> f32 {
        44100.0
    }

    fn set_audio_mixer(
        &mut self,
        _audio_mixer: Arc<fluxemu_frontend_egui::audio::mixer::AudioMixer>,
    ) {
    }
}
