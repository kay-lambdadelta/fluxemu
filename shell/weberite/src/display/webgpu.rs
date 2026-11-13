use fluxemu_frontend::graphics::GraphicsRuntime;
use fluxemu_graphics::{
    api::webgpu::{self, FramebufferRenderer, Webgpu},
    shader::ShaderType,
    texture::CowTexture,
};
use fluxemu_runtime::graphics::{DisplayGeometry, GraphicsRequirements};
use nalgebra::Vector2;
use palette::Srgba;
use pollster::FutureExt;
use raw_window_handle::{RawDisplayHandle, RawWindowHandle, WebDisplayHandle, WebWindowHandle};
use web_sys::HtmlCanvasElement;
use wgpu::{
    Adapter, Color, CommandEncoderDescriptor, CurrentSurfaceTexture, Instance, InstanceDescriptor,
    LoadOp, Operations, RenderPassColorAttachment, RenderPassDescriptor, RequestAdapterOptions,
    StoreOp, Surface, SurfaceConfiguration, SurfaceTargetUnsafe, Texture, TextureViewDescriptor,
};

use crate::display::{CanvasAspectRatio, CanvasGraphicsRuntime, DisplayError};

const CANVAS_RAW_HANDLE: u32 = 1;

pub struct WebgpuGraphicsRuntime {
    canvas: HtmlCanvasElement,
    aspect_ratio: CanvasAspectRatio,
    adapter: Adapter,
    surface: Surface<'static>,
    graphics_requirements: GraphicsRequirements<Webgpu>,
    device_state: DeviceState,
}

struct DeviceState {
    context: webgpu::Context,
    surface_configuration: SurfaceConfiguration,
    renderer: FramebufferRenderer,
}

impl DeviceState {
    fn new(
        adapter: &Adapter,
        surface: &Surface<'static>,
        graphics_requirements: &GraphicsRequirements<Webgpu>,
        dimensions: Vector2<u32>,
    ) -> Result<Self, DisplayError> {
        let (device, queue) = webgpu::request_device(
            adapter,
            &graphics_requirements.required,
            &graphics_requirements.preferred,
        )
        .block_on()?;

        let mut surface_configuration = surface
            .get_default_config(adapter, dimensions.x.max(1), dimensions.y.max(1))
            .ok_or(DisplayError::UnsupportedSurface)?;

        if let Some(format) =
            webgpu::suggested_surface_format(&surface.get_capabilities(adapter).formats)
        {
            surface_configuration.format = format;
        }

        surface.configure(&device, &surface_configuration);

        let renderer = FramebufferRenderer::new(
            device.clone(),
            queue.clone(),
            surface_configuration.format,
            ShaderType::default(),
        );

        Ok(Self {
            context: webgpu::Context { device, queue },
            surface_configuration,
            renderer,
        })
    }
}

impl WebgpuGraphicsRuntime {
    fn canvas_dimensions(&self) -> Vector2<u32> {
        Vector2::new(self.canvas.width(), self.canvas.height())
    }

    fn resize(&mut self, dimensions: Vector2<u32>) {
        self.canvas.set_width(dimensions.x);
        self.canvas.set_height(dimensions.y);

        self.refresh_surface();
    }
}

impl GraphicsRuntime for WebgpuGraphicsRuntime {
    type GraphicsApi = Webgpu;

    fn reconfigure(&mut self, graphics_requirements: GraphicsRequirements<Webgpu>) {
        if graphics_requirements.required == self.graphics_requirements.required
            && graphics_requirements.preferred == self.graphics_requirements.preferred
        {
            return;
        }

        self.device_state = DeviceState::new(
            &self.adapter,
            &self.surface,
            &graphics_requirements,
            self.canvas_dimensions(),
        )
        .expect("Creating device");

        self.graphics_requirements = graphics_requirements;
    }

    fn refresh_surface(&mut self) {
        let dimensions = self.canvas_dimensions();
        let DeviceState {
            context,
            surface_configuration,
            ..
        } = &mut self.device_state;

        surface_configuration.width = dimensions.x.max(1);
        surface_configuration.height = dimensions.y.max(1);

        self.surface
            .configure(&context.device, surface_configuration);
    }

    fn with_context<T>(&mut self, callback: impl FnOnce(&mut webgpu::Context) -> T) -> T {
        callback(&mut self.device_state.context)
    }

    fn max_texture_side(&self) -> u32 {
        self.device_state
            .context
            .device
            .limits()
            .max_texture_dimension_2d
    }

    fn screenshot<'a>(&self, framebuffer: &'a Texture) -> Option<CowTexture<'a, Srgba<u8>>> {
        webgpu::read_framebuffer(&self.device_state.context, framebuffer).map(Into::into)
    }
}

impl CanvasGraphicsRuntime for WebgpuGraphicsRuntime {
    fn new(
        canvas: HtmlCanvasElement,
        graphics_requirements: GraphicsRequirements<Webgpu>,
    ) -> Result<Self, DisplayError> {
        canvas
            .set_attribute("data-raw-handle", &CANVAS_RAW_HANDLE.to_string())
            .map_err(DisplayError::Canvas)?;

        let instance = Instance::new(InstanceDescriptor::new_without_display_handle_from_env());

        let surface = unsafe {
            instance.create_surface_unsafe(SurfaceTargetUnsafe::RawHandle {
                raw_display_handle: Some(RawDisplayHandle::Web(WebDisplayHandle::new())),
                raw_window_handle: RawWindowHandle::Web(WebWindowHandle::new(CANVAS_RAW_HANDLE)),
            })
        }?;

        let adapter = instance
            .request_adapter(&RequestAdapterOptions {
                compatible_surface: Some(&surface),
                ..Default::default()
            })
            .block_on()?;

        tracing::info!("Using graphics adapter {:?}", adapter.get_info());

        let dimensions = Vector2::new(canvas.width(), canvas.height());
        let device_state =
            DeviceState::new(&adapter, &surface, &graphics_requirements, dimensions)?;

        Ok(Self {
            aspect_ratio: CanvasAspectRatio::new(canvas.clone()),
            canvas,
            adapter,
            surface,
            graphics_requirements,
            device_state,
        })
    }

    fn present(
        &mut self,
        framebuffer: &Texture,
        display_geometry: &DisplayGeometry,
        shader: ShaderType,
    ) -> Result<(), DisplayError> {
        let display_rectangle = display_geometry.resolve_display_rectangle(framebuffer);
        let dimensions = display_rectangle.size().map(|side| side as u32);

        if dimensions.min() == 0 {
            return Ok(());
        }

        self.aspect_ratio
            .update(display_geometry.display_size(framebuffer))?;

        if dimensions != self.canvas_dimensions() {
            self.resize(dimensions);
        }

        if self.device_state.renderer.shader() != shader {
            self.device_state.renderer = self.device_state.renderer.with_shader(shader);
        }

        let surface_texture = match self.surface.get_current_texture() {
            CurrentSurfaceTexture::Success(surface_texture)
            | CurrentSurfaceTexture::Suboptimal(surface_texture) => surface_texture,
            _ => {
                self.refresh_surface();
                return Ok(());
            }
        };

        let DeviceState {
            context: webgpu::Context { device, queue },
            renderer,
            ..
        } = &self.device_state;

        let surface_view = surface_texture
            .texture
            .create_view(&TextureViewDescriptor::default());

        let mut encoder = device.create_command_encoder(&CommandEncoderDescriptor { label: None });

        {
            let mut render_pass = encoder.begin_render_pass(&RenderPassDescriptor {
                label: None,
                color_attachments: &[Some(RenderPassColorAttachment {
                    view: &surface_view,
                    resolve_target: None,
                    ops: Operations {
                        load: LoadOp::Clear(Color::BLACK),
                        store: StoreOp::Store,
                    },
                    depth_slice: None,
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });

            renderer.draw(
                &mut render_pass,
                framebuffer,
                display_rectangle,
                dimensions.cast::<f32>(),
            );
        }

        queue.submit([encoder.finish()]);
        queue.present(surface_texture);

        Ok(())
    }
}
