use std::sync::Arc;

use egui::PaintCallbackInfo;
use egui_wgpu::{CallbackResources, CallbackTrait};
use fluxemu_graphics::api::webgpu::FramebufferRenderer;
use fluxemu_math::rectangle::Rectangle;
use nalgebra::Vector2;
use wgpu::{RenderPass, Texture};

mod runtime;

pub use runtime::{WebgpuCompatibleDisplayContext, WebgpuGraphicsRuntime};

pub struct FramebufferCallback {
    framebuffer: Texture,
    display_rectangle: Rectangle<usize>,
    renderer: Arc<FramebufferRenderer>,
}

impl FramebufferCallback {
    pub fn new(
        framebuffer: Texture,
        display_rectangle: Rectangle<usize>,
        renderer: Arc<FramebufferRenderer>,
    ) -> Self {
        Self {
            framebuffer,
            display_rectangle,
            renderer,
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
        self.renderer.draw(
            render_pass,
            &self.framebuffer,
            self.display_rectangle,
            Vector2::new(info.viewport.width(), info.viewport.height()),
        );
    }
}
