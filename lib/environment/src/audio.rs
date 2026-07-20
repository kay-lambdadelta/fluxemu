use serde::{Deserialize, Serialize};
use serde_with::serde_as;

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
/// Interpolation settings the audio backend should use
pub enum Interpolation {
    /// Linear interpolation, lowest quality
    Linear,
    /// Cubic interpolation, mid quality
    #[default]
    Cubic,
    /// Sinc interpolation, highest quality
    Sinc {
        /// Number of taps to use
        taps: u8,
    },
}

#[serde_as]
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct AudioSettings {
    /// Interpolation settings
    pub interpolation: Interpolation,
}
