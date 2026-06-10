use std::{fmt::Debug, sync::Arc};

use crate::audio::mixer::AudioMixer;

pub mod mixer;

/// Audio runtime to provide the frontend
pub trait AudioRuntime: Sized + Debug {
    /// Retrieve the sample rate
    fn sample_rate(&mut self) -> f32;
    /// Set the audio mixer
    fn set_audio_mixer(&mut self, audio_mixer: Arc<AudioMixer>);
    /// Pause audio playback
    fn pause(&mut self);
    /// Play audio
    fn play(&mut self);
}
