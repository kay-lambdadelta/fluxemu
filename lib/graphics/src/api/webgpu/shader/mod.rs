use bytemuck::{Pod, Zeroable};
use nalgebra::Vector2;

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub struct ShaderUniform {
    pub viewport_size: Vector2<f32>,
    pub framebuffer_size: Vector2<f32>,
}

pub const NORMAL_SHADER: &str = include_str!("normal.wgsl");
