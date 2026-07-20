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
    /// Api to use
    pub api: GraphicsApi,
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum OverlayDisableBehavior {
    /// This bypasses the ui machinery when the overlay is disabled. This may gain performance at the costs of toasts being nonvisible
    Bypass,
    /// Go through the ui machinery in order to draw toasts over an active game when the overlay is disabled
    #[default]
    Toasts,
}
