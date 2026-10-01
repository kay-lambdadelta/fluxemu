use egui::{PaintCallback, Rect};
use fluxemu_graphics::texture::RefMutTexture;
use std::{fmt::Debug, sync::Arc};

pub trait CallbackTrait<P>: Send + Sync + 'static {
    fn paint(&self, callback_info: CallbackInfo, texture: RefMutTexture<'_, P>);
}

impl<P, F: Fn(CallbackInfo, RefMutTexture<'_, P>) + Send + Sync + 'static> CallbackTrait<P> for F {
    fn paint(&self, callback_info: CallbackInfo, texture: RefMutTexture<'_, P>) {
        self(callback_info, texture);
    }
}

pub struct Callback<P>(pub(crate) Box<dyn CallbackTrait<P>>);

impl<P: 'static> Callback<P> {
    pub fn new_paint_callback(rect: Rect, callback: impl CallbackTrait<P>) -> PaintCallback {
        PaintCallback {
            rect,
            callback: Arc::new(Self(Box::new(callback))),
        }
    }
}

impl<P> Debug for Callback<P> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CallbackWrapper").finish_non_exhaustive()
    }
}

#[derive(Debug, Clone, Copy)]
pub struct CallbackInfo {
    pub pixels_per_point: f32,
}
