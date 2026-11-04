use std::borrow::Cow;

use fluxemu_frontend::InputRouting;
use fluxemu_input::{
    InputId, InputState,
    physical::{PhysicalInputDeviceInstanceId, PhysicalInputDeviceKind},
};

use crate::{Frontend, Platform};

impl<P: Platform> Frontend<P> {
    pub fn insert_input(
        &mut self,
        origin: PhysicalInputDeviceInstanceId,
        input_id: InputId,
        state: InputState,
    ) {
        // Make sure our main loop is ran
        self.egui_context.request_repaint();

        let machine_has_focus = self.machine_has_focus();

        if let InputRouting::Ui = self
            .core
            .insert_input(origin, input_id, state, machine_has_focus)
        {
            self.egui_input_translator
                .insert_input(&self.egui_context, input_id, state);
        }

        self.handle_core_events();
    }

    pub fn register_gamepad(
        &mut self,
        id: PhysicalInputDeviceInstanceId,
        name: impl Into<Cow<'static, str>>,
        kind: PhysicalInputDeviceKind,
        rely_on_frontend_input_handling: bool,
    ) {
        // Make sure our main loop is ran
        self.egui_context.request_repaint();

        self.core
            .register_gamepad(id, name, kind, rely_on_frontend_input_handling);
    }

    pub fn unregister_gamepad(&mut self, id: PhysicalInputDeviceInstanceId) {
        self.egui_context.request_repaint();

        self.core.unregister_gamepad(id);
        self.egui_input_translator.release_all();
    }
}
