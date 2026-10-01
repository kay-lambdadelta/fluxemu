use fluxemu_egui_software_renderer::callback::{CallbackInfo, CallbackTrait};
use fluxemu_graphics::texture::{CopyMode, OwnedTexture, RefMutTexture};

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
