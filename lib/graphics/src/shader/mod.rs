extern crate std;

use std::{collections::HashMap, sync::LazyLock};

use serde::{Deserialize, Serialize};
use strum::{Display, EnumIter};

#[derive(Copy, Clone, Hash, PartialEq, Eq)]
pub enum ShaderLanguage {
    Wgsl,
}

#[derive(
    Serialize, Deserialize, Debug, Copy, Clone, Hash, PartialEq, Eq, Default, EnumIter, Display,
)]
pub enum ShaderType {
    #[default]
    Standard,
    Scanline,
}

pub static BUILTIN_SHADERS: LazyLock<HashMap<ShaderType, HashMap<ShaderLanguage, &'static str>>> =
    LazyLock::new(|| {
        HashMap::from_iter([
            (
                ShaderType::Standard,
                HashMap::from_iter([(ShaderLanguage::Wgsl, include_str!("wgsl/standard.wgsl"))]),
            ),
            (
                ShaderType::Scanline,
                HashMap::from_iter([(ShaderLanguage::Wgsl, include_str!("wgsl/scanline.wgsl"))]),
            ),
        ])
    });
