use fluxemu_math::rectangle::Rectangle;
use nalgebra::Vector2;
use wgpu::{
    BindGroupDescriptor, BindGroupEntry, BindGroupLayout, BindGroupLayoutDescriptor,
    BindGroupLayoutEntry, BindingResource, BindingType, BlendState, Buffer, BufferBindingType,
    BufferDescriptor, BufferUsages, ColorTargetState, ColorWrites, Device, FilterMode,
    FragmentState, MultisampleState, PipelineCompilationOptions, PipelineLayoutDescriptor,
    PrimitiveState, Queue, RenderPass, RenderPipeline, RenderPipelineDescriptor, Sampler,
    SamplerBindingType, SamplerDescriptor, ShaderModuleDescriptor, ShaderSource, ShaderStages,
    Texture, TextureFormat, TextureSampleType, TextureViewDescriptor, TextureViewDimension,
    VertexState,
};

use crate::{
    api::webgpu::shader::ShaderUniform,
    shader::{BUILTIN_SHADERS, ShaderLanguage, ShaderType},
};

#[derive(Debug)]
pub struct FramebufferRenderer {
    bind_group_layout: BindGroupLayout,
    pipeline: RenderPipeline,
    uniform_buffer: Buffer,
    sampler: Sampler,
    device: Device,
    queue: Queue,
    view_format: TextureFormat,
    shader: ShaderType,
}

impl FramebufferRenderer {
    pub fn new(
        device: Device,
        queue: Queue,
        view_format: TextureFormat,
        shader: ShaderType,
    ) -> Self {
        let shader_module = device.create_shader_module(ShaderModuleDescriptor {
            label: None,
            source: ShaderSource::Wgsl(BUILTIN_SHADERS[&shader][&ShaderLanguage::Wgsl].into()),
        });

        let bind_group_layout = device.create_bind_group_layout(&BindGroupLayoutDescriptor {
            label: None,
            entries: &[
                BindGroupLayoutEntry {
                    binding: 0,
                    visibility: ShaderStages::VERTEX_FRAGMENT,
                    ty: BindingType::Buffer {
                        ty: BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                BindGroupLayoutEntry {
                    binding: 1,
                    visibility: ShaderStages::FRAGMENT,
                    ty: BindingType::Texture {
                        multisampled: false,
                        view_dimension: TextureViewDimension::D2,
                        sample_type: TextureSampleType::Float { filterable: true },
                    },
                    count: None,
                },
                BindGroupLayoutEntry {
                    binding: 2,
                    visibility: ShaderStages::FRAGMENT,
                    ty: BindingType::Sampler(SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });

        let pipeline_layout = device.create_pipeline_layout(&PipelineLayoutDescriptor {
            label: None,
            bind_group_layouts: &[Some(&bind_group_layout)],
            immediate_size: 0,
        });

        let pipeline = device.create_render_pipeline(&RenderPipelineDescriptor {
            label: None,
            layout: Some(&pipeline_layout),
            vertex: VertexState {
                module: &shader_module,
                entry_point: Some("vs_main"),
                buffers: &[],
                compilation_options: PipelineCompilationOptions::default(),
            },
            fragment: Some(FragmentState {
                module: &shader_module,
                entry_point: Some("fs_main"),
                targets: &[Some(ColorTargetState {
                    format: view_format,
                    blend: Some(BlendState::REPLACE),
                    write_mask: ColorWrites::ALL,
                })],
                compilation_options: PipelineCompilationOptions::default(),
            }),
            primitive: PrimitiveState::default(),
            depth_stencil: None,
            multisample: MultisampleState::default(),
            multiview_mask: None,
            cache: None,
        });

        let uniform_buffer = device.create_buffer(&BufferDescriptor {
            label: None,
            size: size_of::<ShaderUniform>() as u64,
            usage: BufferUsages::UNIFORM | BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let sampler = device.create_sampler(&SamplerDescriptor {
            mag_filter: FilterMode::Nearest,
            min_filter: FilterMode::Linear,
            ..Default::default()
        });

        FramebufferRenderer {
            bind_group_layout,
            pipeline,
            uniform_buffer,
            sampler,
            device,
            queue,
            view_format,
            shader,
        }
    }

    pub fn shader(&self) -> ShaderType {
        self.shader
    }

    pub fn with_shader(&self, shader: ShaderType) -> Self {
        Self::new(
            self.device.clone(),
            self.queue.clone(),
            self.view_format,
            shader,
        )
    }

    pub fn draw(
        &self,
        render_pass: &mut RenderPass<'_>,
        framebuffer: &Texture,
        display_rectangle: Rectangle<usize>,
        viewport_size: Vector2<f32>,
    ) {
        render_pass.set_pipeline(&self.pipeline);

        let texture_view = framebuffer.create_view(&TextureViewDescriptor::default());

        let size = framebuffer.size();
        let framebuffer_size = Vector2::new(size.width as f32, size.height as f32);

        let uniforms = ShaderUniform {
            viewport_size,
            framebuffer_size,
            display_minimum_uv: display_rectangle
                .min
                .coords
                .cast::<f32>()
                .component_div(&framebuffer_size),
            display_maximum_uv: display_rectangle
                .max
                .coords
                .cast::<f32>()
                .component_div(&framebuffer_size),
        };

        self.queue
            .write_buffer(&self.uniform_buffer, 0, bytemuck::bytes_of(&uniforms));

        let bind_group = self.device.create_bind_group(&BindGroupDescriptor {
            label: None,
            layout: &self.bind_group_layout,
            entries: &[
                BindGroupEntry {
                    binding: 0,
                    resource: self.uniform_buffer.as_entire_binding(),
                },
                BindGroupEntry {
                    binding: 1,
                    resource: BindingResource::TextureView(&texture_view),
                },
                BindGroupEntry {
                    binding: 2,
                    resource: BindingResource::Sampler(&self.sampler),
                },
            ],
        });

        render_pass.set_bind_group(0, &bind_group, &[]);
        render_pass.draw(0..3, 0..1);
    }
}
