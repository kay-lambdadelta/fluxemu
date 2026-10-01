use std::sync::Arc;

use egui::PaintCallbackInfo;
use egui_wgpu::{CallbackResources, CallbackTrait};
use fluxemu_graphics::api::webgpu::shader::{NORMAL_SHADER, ShaderUniform};
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

#[derive(Debug)]
pub struct Resources {
    bind_group_layout: BindGroupLayout,
    pipeline: RenderPipeline,
    uniform_buffer: Buffer,
    sampler: Sampler,
    device: Device,
    queue: Queue,
}

impl Resources {
    pub fn new(device: Device, queue: Queue, view_format: TextureFormat) -> Arc<Self> {
        let shader = device.create_shader_module(ShaderModuleDescriptor {
            label: None,
            source: ShaderSource::Wgsl(NORMAL_SHADER.into()),
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
                module: &shader,
                entry_point: Some("vs_main"),
                buffers: &[],
                compilation_options: PipelineCompilationOptions::default(),
            },
            fragment: Some(FragmentState {
                module: &shader,
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

        Arc::new(Resources {
            bind_group_layout,
            pipeline,
            uniform_buffer,
            sampler,
            device,
            queue,
        })
    }
}

pub struct FramebufferCallback {
    framebuffer: Texture,
    resources: Arc<Resources>,
}

impl FramebufferCallback {
    pub fn new(framebuffer: Texture, resources: Arc<Resources>) -> Self {
        Self {
            framebuffer,
            resources,
        }
    }
}

impl CallbackTrait for FramebufferCallback {
    fn paint(
        &self,
        info: PaintCallbackInfo,
        render_pass: &mut RenderPass<'static>,
        _callback_resources: &CallbackResources,
    ) {
        render_pass.set_pipeline(&self.resources.pipeline);

        let texture_view = self
            .framebuffer
            .create_view(&TextureViewDescriptor::default());

        let size = self.framebuffer.size();

        let uniforms = ShaderUniform {
            viewport_size: Vector2::new(info.viewport.width(), info.viewport.height()),
            framebuffer_size: Vector2::new(size.width as f32, size.height as f32),
        };

        self.resources.queue.write_buffer(
            &self.resources.uniform_buffer,
            0,
            bytemuck::bytes_of(&uniforms),
        );

        let bind_group = self
            .resources
            .device
            .create_bind_group(&BindGroupDescriptor {
                label: None,
                layout: &self.resources.bind_group_layout,
                entries: &[
                    BindGroupEntry {
                        binding: 0,
                        resource: self.resources.uniform_buffer.as_entire_binding(),
                    },
                    BindGroupEntry {
                        binding: 1,
                        resource: BindingResource::TextureView(&texture_view),
                    },
                    BindGroupEntry {
                        binding: 2,
                        resource: BindingResource::Sampler(&self.resources.sampler),
                    },
                ],
            });

        render_pass.set_bind_group(0, &bind_group, &[]);
        render_pass.draw(0..3, 0..1);
    }
}
