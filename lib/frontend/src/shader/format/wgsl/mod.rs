use std::sync::Arc;

use fluxemu_graphics::shader::{ShaderLanguage, ShaderSource};
use naga::{Module, back, front};

use crate::shader::{ShaderError, ShaderFormat, ShaderModules, format::validate};

pub struct Wgsl;

impl ShaderFormat for Wgsl {
    const LANGUAGE: ShaderLanguage = ShaderLanguage::Wgsl;

    fn to_module(shader: &ShaderSource) -> Result<ShaderModules, ShaderError> {
        let vertex = parse_wgsl(&shader.vertex)?;
        let fragment = if shader.is_shared() {
            vertex.clone()
        } else {
            parse_wgsl(&shader.fragment)?
        };

        Ok(ShaderModules { vertex, fragment })
    }

    fn from_module(modules: &ShaderModules) -> Result<ShaderSource, ShaderError> {
        Ok(ShaderSource {
            vertex: write_wgsl(&modules.vertex)?,
            fragment: write_wgsl(&modules.fragment)?,
        })
    }

    fn to_file(shader: &ShaderSource) -> Result<String, ShaderError> {
        if !shader.is_shared() {
            return Err(ShaderError::SplitStages(Self::LANGUAGE));
        }

        Ok(shader.vertex.to_string())
    }
}

fn parse_wgsl(source: &str) -> Result<Module, ShaderError> {
    front::wgsl::parse_str(source)
        .map_err(|error| ShaderError::Parse(Wgsl::LANGUAGE, error.emit_to_string(source)))
}

fn write_wgsl(module: &Module) -> Result<Arc<str>, ShaderError> {
    let info = validate(module)?;

    Ok(back::wgsl::write_string(module, &info, back::wgsl::WriterFlags::empty())?.into())
}
