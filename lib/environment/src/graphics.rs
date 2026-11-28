use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use strum::{Display, EnumIter};

/// Graphics API the graphics runtime will use
#[non_exhaustive]
#[derive(Serialize, Deserialize, Debug, Clone, Copy, EnumIter, Display, PartialEq, Eq, Default)]
pub enum GraphicsApi {
    /// Software rendering
    #[cfg_attr(
        not(all(
            any(target_family = "unix", target_os = "windows"),
            not(target_os = "horizon")
        )),
        default
    )]
    Software,
    #[cfg_attr(
        all(
            any(target_family = "unix", target_os = "windows"),
            not(target_os = "horizon")
        ),
        default
    )]
    Webgpu,
}

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct GraphicsSettings {
    /// When scaling the display buffer to the render surface, should fractional
    /// scaling be disabled?
    pub integer_scaling: bool,
    /// API to use
    pub api: GraphicsApi,
    /// Shader the UI is currently using
    #[serde(default)]
    pub shader: PathBuf,
}
