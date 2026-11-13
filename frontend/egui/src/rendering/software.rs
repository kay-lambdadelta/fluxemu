use egui::PaintCallback;
use fluxemu_egui_software_renderer::{
    Renderer,
    callback::{Callback, CallbackInfo, CallbackTrait},
};
use fluxemu_frontend::graphics::{DisplayContext, GraphicsRuntime};
use fluxemu_graphics::{
    api::{GraphicsApi, software::Software},
    shader::ShaderType,
    texture::{AsViewTexture, AsViewTextureMut, CopyMode, CowTexture, OwnedTexture, RefMutTexture},
};
use fluxemu_math::rectangle::Rectangle;
use fluxemu_runtime::graphics::GraphicsRequirements;
use palette::{Srgb, Srgba, named::BLACK};

use crate::rendering::EguiCapableGraphicsRuntime;

pub struct FramebufferCallback<P> {
    framebuffer: OwnedTexture<P>,
}

impl<P> FramebufferCallback<P> {
    pub fn new(framebuffer: OwnedTexture<P>) -> Self {
        Self { framebuffer }
    }
}

impl<P: Clone + Send + Sync + 'static> CallbackTrait<P> for FramebufferCallback<P> {
    fn paint(&self, _callback_info: CallbackInfo, mut texture: RefMutTexture<'_, P>) {
        texture.copy_from(&self.framebuffer, CopyMode::Nearest);
    }
}

pub trait SoftwareCompatibleDisplayContext: DisplayContext {
    type Pixel: From<Srgba<u8>> + Into<Srgba<u8>> + Send + Sync + Copy + 'static;
    type Surface;
    type ResizeError: std::error::Error;
    type MappingError: std::error::Error;
    type PresentError: std::error::Error;

    fn resize_surface(&self, surface: &mut Self::Surface) -> Result<(), Self::ResizeError>;

    fn map_surface_buffer<'a>(
        &'a self,
        surface: &'a mut Self::Surface,
    ) -> Result<impl AsViewTextureMut<Self::Pixel> + 'a, Self::MappingError>;

    fn present(&self, surface: &mut Self::Surface) -> Result<(), Self::PresentError>;
}

// Benchmarks say that a batch size of 16 is ideal across several low and mid power machines
//
// As far as throughput for realistic UI goes at the very least
//
// This is suggested by benchmarks on a i5-1245U and a RK3566T
pub struct SoftwareGraphicsRuntime<
    D: SoftwareCompatibleDisplayContext,
    const BATCH_SIZE: usize = 16,
> {
    renderer: Renderer,
    surface: D::Surface,
    display_context: D,
}

impl<D: SoftwareCompatibleDisplayContext, const BATCH_SIZE: usize>
    SoftwareGraphicsRuntime<D, BATCH_SIZE>
{
    pub fn new(display_context: D, surface: D::Surface) -> Self {
        Self {
            renderer: Renderer::default(),
            surface,
            display_context,
        }
    }

    pub fn display_context(&self) -> &D {
        &self.display_context
    }
}

impl<D: SoftwareCompatibleDisplayContext, const BATCH_SIZE: usize> GraphicsRuntime
    for SoftwareGraphicsRuntime<D, BATCH_SIZE>
{
    type GraphicsApi = Software;

    fn reconfigure(&mut self, _graphics_requirements: GraphicsRequirements<Self::GraphicsApi>) {
        // Nothing. Software backend is completely static
    }

    fn refresh_surface(&mut self) {
        self.display_context
            .resize_surface(&mut self.surface)
            .unwrap();
    }

    fn with_context<T>(&mut self, callback: impl FnOnce(&mut ()) -> T) -> T {
        callback(&mut ())
    }

    fn max_texture_side(&self) -> u32 {
        self.renderer.max_texture_size()
    }

    fn screenshot<'a>(
        &self,
        framebuffer: &'a <Self::GraphicsApi as GraphicsApi>::Framebuffer,
    ) -> Option<CowTexture<'a, Srgba<u8>>> {
        Some(framebuffer.as_view().into())
    }
}

impl<D: SoftwareCompatibleDisplayContext, const BATCH_SIZE: usize> EguiCapableGraphicsRuntime
    for SoftwareGraphicsRuntime<D, BATCH_SIZE>
{
    fn present(
        &mut self,
        context: &egui::Context,
        clear_color: Srgb<u8>,
        full_output: egui::FullOutput,
    ) {
        let mut surface_buffer_guard = self
            .display_context
            .map_surface_buffer(&mut self.surface)
            .unwrap();

        let mut surface_buffer = surface_buffer_guard.as_view_mut();

        surface_buffer.fill(Srgba::from(clear_color).into());

        self.renderer
            .render::<_, BATCH_SIZE>(context, full_output, &mut surface_buffer);

        drop(surface_buffer_guard);

        self.display_context.pre_present_notify();
        self.display_context.present(&mut self.surface).unwrap();
    }

    fn produce_callback_for_framebuffer(
        &mut self,
        rect: egui::Rect,
        framebuffer: &<Self::GraphicsApi as GraphicsApi>::Framebuffer,
        display_rectangle: Rectangle<usize>,
        _shader: ShaderType,
    ) -> PaintCallback {
        let framebuffer =
            framebuffer.view(display_rectangle.x_range(), display_rectangle.y_range());

        let mut converted_framebuffer = OwnedTexture::<D::Pixel>::from_value(
            framebuffer.width(),
            framebuffer.height(),
            Srgba::from(BLACK).into(),
        );
        converted_framebuffer.map_from(framebuffer, CopyMode::Nearest, From::from);

        let callback = FramebufferCallback::new(converted_framebuffer);

        Callback::new_paint_callback(rect, callback)
    }
}
