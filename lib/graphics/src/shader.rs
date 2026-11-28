use alloc::sync::Arc;

use strum::{EnumIter, IntoStaticStr};

#[derive(Debug, Copy, Clone, Hash, PartialEq, Eq, IntoStaticStr, EnumIter)]
#[strum(serialize_all = "kebab_case")]
pub enum ShaderLanguage {
    Wgsl,
    Glsl,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShaderSource {
    pub vertex: Arc<str>,
    pub fragment: Arc<str>,
}

impl ShaderSource {
    pub fn new(source: Arc<str>) -> Self {
        Self {
            vertex: source.clone(),
            fragment: source,
        }
    }

    pub fn is_shared(&self) -> bool {
        Arc::ptr_eq(&self.vertex, &self.fragment)
    }
}
