use std::{
    cell::{OnceCell, RefCell},
    rc::Rc,
};

use wasm_bindgen::{JsCast, JsValue, closure::Closure};
use web_sys::Window;

use crate::{Application, display::CanvasGraphicsRuntime};

type FrameCallback = Rc<OnceCell<Closure<dyn FnMut()>>>;

pub fn start_frame_loop<R: CanvasGraphicsRuntime>(
    application: &Rc<RefCell<Application<R>>>,
) -> Result<(), JsValue> {
    let window = web_sys::window().ok_or("This environment has no window")?;
    let callback = FrameCallback::default();

    let application = application.clone();
    let callback_window = window.clone();
    let inner_callback = callback.clone();
    let _ = callback.set(Closure::<dyn FnMut()>::own_aborting(move || {
        application.borrow_mut().frame();

        if let Err(err) = request_frame(&callback_window, &inner_callback) {
            tracing::error!("Frame loop stopped: {:?}", err);
        }
    }));

    request_frame(&window, &callback)
}

fn request_frame(window: &Window, callback: &FrameCallback) -> Result<(), JsValue> {
    let callback = callback.get().ok_or("Frame callback is missing")?;

    window.request_animation_frame(callback.as_ref().unchecked_ref())?;

    Ok(())
}
