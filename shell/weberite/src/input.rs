use std::{cell::RefCell, rc::Rc};

use fluxemu_input::{
    InputId, InputState,
    physical::{PhysicalInputDeviceInstanceId, PhysicalInputDeviceKind},
};
use keyboard_types::Code;
use wasm_bindgen::{JsCast, JsValue, closure::Closure};
use web_sys::KeyboardEvent;

use crate::Application;

const KEYBOARD: PhysicalInputDeviceInstanceId = PhysicalInputDeviceInstanceId::PLATFORM_RESERVED;

pub fn register_keyboard(application: &Rc<RefCell<Application>>) -> Result<(), JsValue> {
    let window = web_sys::window().ok_or("This environment has no window")?;

    application.borrow_mut().core.register_gamepad(
        KEYBOARD,
        "Keyboard",
        PhysicalInputDeviceKind::Keyboard,
        false,
    );

    for (event_name, state) in [
        ("keydown", InputState::PRESSED),
        ("keyup", InputState::RELEASED),
    ] {
        let application = application.clone();
        let listener =
            Closure::<dyn FnMut(KeyboardEvent)>::own_aborting(move |event: KeyboardEvent| {
                if event.repeat() {
                    return;
                }

                let Ok(code) = event.code().parse::<Code>() else {
                    return;
                };

                application.borrow_mut().core.insert_input(
                    KEYBOARD,
                    InputId::Keyboard(code),
                    state,
                    true,
                );
            });

        window.add_event_listener_with_callback(event_name, listener.as_ref().unchecked_ref())?;
        listener.forget();
    }

    Ok(())
}
