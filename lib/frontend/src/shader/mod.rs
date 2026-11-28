use std::{
    collections::HashMap,
    path::Path,
    sync::{Arc, LazyLock},
};

use fluxemu_graphics::shader::{ShaderLanguage, ShaderSource};
use strum::{EnumIter, IntoEnumIterator, IntoStaticStr};

pub(crate) use self::manager::{ShaderManager, ShaderScan};
pub use self::{
    format::{Glsl, ShaderFormat, ShaderModules, Wgsl, convert, to_file},
    manager::ShaderError,
};

mod format;
mod manager;

#[derive(Debug, Copy, Clone, Hash, PartialEq, Eq, Default, EnumIter, IntoStaticStr)]
#[strum(serialize_all = "kebab_case")]
pub(crate) enum ShaderType {
    #[default]
    Standard,
    Scanline,
}

pub(crate) static BUILTIN_SHADERS: LazyLock<
    HashMap<ShaderLanguage, HashMap<ShaderType, &'static str>>,
> = LazyLock::new(|| {
    HashMap::from_iter([(
        ShaderLanguage::Wgsl,
        HashMap::from_iter([
            (
                ShaderType::Standard,
                include_str!("format/wgsl/standard.wgsl"),
            ),
            (
                ShaderType::Scanline,
                include_str!("format/wgsl/scanline.wgsl"),
            ),
        ]),
    )])
});

pub(crate) fn builtin_shader(
    language: ShaderLanguage,
    shader_type: ShaderType,
) -> Option<ShaderSource> {
    BUILTIN_SHADERS
        .get(&language)
        .and_then(|shaders| shaders.get(&shader_type))
        .map(|source| ShaderSource::new(Arc::from(*source)))
}

pub(crate) fn shader_type_by_name(name: &Path) -> Option<ShaderType> {
    ShaderType::iter().find(|shader_type| name == Path::new(<&str>::from(*shader_type)))
}
