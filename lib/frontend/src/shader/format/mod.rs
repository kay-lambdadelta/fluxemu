use fluxemu_graphics::shader::{ShaderLanguage, ShaderSource};
pub use glsl::Glsl;
use naga::{
    Module,
    valid::{Capabilities, ModuleInfo, ValidationFlags, Validator},
};
pub use wgsl::Wgsl;

use crate::shader::ShaderError;

mod glsl;
mod wgsl;

pub struct ShaderModules {
    pub vertex: Module,
    pub fragment: Module,
}

pub trait ShaderFormat {
    const LANGUAGE: ShaderLanguage;

    fn to_module(shader: &ShaderSource) -> Result<ShaderModules, ShaderError>;
    fn from_module(modules: &ShaderModules) -> Result<ShaderSource, ShaderError>;
    fn to_file(shader: &ShaderSource) -> Result<String, ShaderError>;
}

fn validate(module: &Module) -> Result<ModuleInfo, ShaderError> {
    Validator::new(ValidationFlags::all(), Capabilities::all())
        .validate(module)
        .map_err(|error| ShaderError::Validation(error.into_inner().to_string()))
}

pub fn convert(
    from: ShaderLanguage,
    to: ShaderLanguage,
    shader: &ShaderSource,
) -> Result<ShaderSource, ShaderError> {
    let modules = match from {
        ShaderLanguage::Wgsl => Wgsl::to_module(shader)?,
        ShaderLanguage::Glsl => Glsl::to_module(shader)?,
    };

    match to {
        ShaderLanguage::Wgsl => Wgsl::from_module(&modules),
        ShaderLanguage::Glsl => Glsl::from_module(&modules),
    }
}

pub fn to_file(language: ShaderLanguage, shader: &ShaderSource) -> Result<String, ShaderError> {
    match language {
        ShaderLanguage::Wgsl => Wgsl::to_file(shader),
        ShaderLanguage::Glsl => Glsl::to_file(shader),
    }
}
