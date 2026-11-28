use std::sync::Arc;

use fluxemu_graphics::shader::{ShaderLanguage, ShaderSource};
use naga::{
    Module, ShaderStage,
    back::{
        self,
        glsl::{BindingMap, PipelineOptions, Version},
    },
    front,
    proc::BoundsCheckPolicies,
};

use crate::shader::{ShaderError, ShaderFormat, ShaderModules, format::validate};

pub struct Glsl;

const GLSL_VERSION: Version = Version::Desktop(450);

impl ShaderFormat for Glsl {
    const LANGUAGE: ShaderLanguage = ShaderLanguage::Glsl;

    fn to_module(shader: &ShaderSource) -> Result<ShaderModules, ShaderError> {
        Ok(ShaderModules {
            vertex: parse_glsl(&shader.vertex, ShaderStage::Vertex)?,
            fragment: parse_glsl(&shader.fragment, ShaderStage::Fragment)?,
        })
    }

    fn from_module(modules: &ShaderModules) -> Result<ShaderSource, ShaderError> {
        Ok(ShaderSource {
            vertex: write_glsl(&modules.vertex, ShaderStage::Vertex)?,
            fragment: write_glsl(&modules.fragment, ShaderStage::Fragment)?,
        })
    }

    fn to_file(shader: &ShaderSource) -> Result<String, ShaderError> {
        if shader.is_shared() {
            return Ok(shader.vertex.to_string());
        }

        let (version, vertex) = split_version(&shader.vertex);
        let (_, fragment) = split_version(&shader.fragment);

        Ok(format!(
            "{version}\n#ifdef VERTEX\n\n{vertex}\n\n#endif\n#ifdef FRAGMENT\n\n{fragment}\n\n#endif\n"
        ))
    }
}

fn split_version(source: &str) -> (&str, &str) {
    match source.split_once('\n') {
        Some((first_line, rest)) if first_line.trim_start().starts_with("#version") => {
            (first_line, rest)
        }
        _ => ("", source),
    }
}

fn parse_glsl(source: &str, stage: ShaderStage) -> Result<Module, ShaderError> {
    let define = match stage {
        ShaderStage::Vertex => "VERTEX",
        ShaderStage::Fragment => "FRAGMENT",
        _ => unreachable!(),
    };

    let options = front::glsl::Options {
        stage,
        defines: [(define.to_string(), "1".to_string())]
            .into_iter()
            .collect(),
    };

    front::glsl::Frontend::default()
        .parse(&options, source)
        .map_err(|error| ShaderError::Parse(Glsl::LANGUAGE, error.emit_to_string(source)))
}

fn write_glsl(module: &Module, stage: ShaderStage) -> Result<Arc<str>, ShaderError> {
    let info = validate(module)?;

    let entry_point = module
        .entry_points
        .iter()
        .find(|entry_point| entry_point.stage == stage)
        .ok_or(ShaderError::MissingEntryPoint(stage))?;

    let mut resource_bindings: Vec<_> = module
        .global_variables
        .iter()
        .filter_map(|(_, global)| global.binding)
        .collect();
    resource_bindings.sort();
    resource_bindings.dedup();

    let binding_map: BindingMap = resource_bindings.into_iter().zip(0..).collect();

    let options = back::glsl::Options {
        version: GLSL_VERSION,
        writer_flags: back::glsl::WriterFlags::empty(),
        binding_map,
        ..Default::default()
    };

    let pipeline_options = PipelineOptions {
        shader_stage: stage,
        entry_point: entry_point.name.clone(),
        multiview: None,
    };

    let mut output = String::new();

    back::glsl::Writer::new(
        &mut output,
        module,
        &info,
        &options,
        &pipeline_options,
        BoundsCheckPolicies::default(),
    )?
    .write()?;

    Ok(output.into())
}
