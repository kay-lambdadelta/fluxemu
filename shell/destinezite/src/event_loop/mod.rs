use clap::ValueEnum;

#[cfg(feature = "drm")]
pub mod drm;
#[cfg(feature = "windowing")]
pub mod windowing;

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum, strum::Display, Default)]
#[strum(serialize_all = "kebab_case")]
#[clap(rename_all = "kebab_case")]
pub enum DisplayBackend {
    #[cfg(feature = "windowing")]
    #[cfg_attr(feature = "windowing", default)]
    Windowing,
    #[cfg(feature = "drm")]
    #[cfg_attr(not(feature = "windowing"), default)]
    Drm,
}
