use std::borrow::Cow;

use egui_toast::ToastKind;
use fluxemu_input::{
    InputId, InputState,
    physical::{PhysicalInputDeviceId, hotkey::Hotkey},
};
use indexmap::{IndexMap, IndexSet};

use crate::{Frontend, FrontendPlatform, MachineContext, PhysicalInputDeviceState};

impl<P: FrontendPlatform> Frontend<P> {
    pub fn insert_input(
        &mut self,
        origin: PhysicalInputDeviceId,
        input_id: InputId,
        state: InputState,
    ) {
        // Make sure our main loop is ran
        self.egui_context.request_repaint();

        let Some(physical_input_device_state) = self.physical_input_devices.get_mut(&origin) else {
            tracing::error!("Ignoring unknown device {}", origin);

            return;
        };

        tracing::trace!(
            ?origin,
            ?input_id,
            ?state,
            "{}",
            physical_input_device_state.name
        );

        let physical_gamepad_configuration = self.environment.gamepads.entry(origin).or_default();

        physical_input_device_state
            .gui_relevant_input_state
            .insert(input_id, state);

        let mut was_relevant_for_hotkeys = false;

        // Check for hotkeys
        for (combinations, hotkey_action) in &physical_gamepad_configuration.hotkey {
            let is_activated = combinations.iter().all(|input_id| {
                physical_input_device_state
                    .gui_relevant_input_state
                    .get(input_id)
                    .copied()
                    .unwrap_or(InputState::RELEASED)
                    .as_digital(None)
            });

            if is_activated {
                was_relevant_for_hotkeys = true;

                match hotkey_action {
                    Hotkey::ToggleMenu => {
                        if self.frontend_overlay_active {
                            if let Some(MachineContext {
                                simulation_controller,
                                ..
                            }) = &mut self.machine_context
                            {
                                // We don't allow the overlay to be deactivated if there isn't an active machine
                                self.frontend_overlay_active = false;
                                simulation_controller.set_paused(false);
                            }
                        } else {
                            // Pause machine if one is active
                            if let Some(MachineContext {
                                simulation_controller,
                                machine,
                                ..
                            }) = &mut self.machine_context
                            {
                                // Enter runtime
                                let runtime_guard = machine.enter_runtime();

                                // Unset ALL inputs
                                for (logical_input_device_path, logical_input_device) in
                                    runtime_guard.input_devices()
                                {
                                    let unset_inputs = logical_input_device
                                        .metadata()
                                        .present_inputs
                                        .iter()
                                        .copied()
                                        .map(|input_id| (input_id, InputState::RELEASED));

                                    runtime_guard
                                        .insert_inputs(logical_input_device_path, unset_inputs);
                                }

                                // Pause machine if one exists
                                simulation_controller.set_paused(true);
                            }

                            self.frontend_overlay_active = true;
                        }
                    }
                    Hotkey::FastForward => {}
                    Hotkey::LoadSnapshot => {}
                    Hotkey::StoreSnapshot => {}
                    Hotkey::IncrementSnapshotCounter => {
                        self.environment.active_snapshot_slot += 1;

                        self.toast_manager.toast(
                            ToastKind::Info,
                            format!(
                                "Active snapshot slot is now {}",
                                self.environment.active_snapshot_slot
                            ),
                        );
                    }
                    Hotkey::DecrementSnapshotCounter => {
                        self.environment.active_snapshot_slot -= 1;

                        self.toast_manager.toast(
                            ToastKind::Info,
                            format!(
                                "Current snapshot slot is now {}",
                                self.environment.active_snapshot_slot
                            ),
                        );
                    }
                }
            }
        }

        // Ignore if that key participated in a hotkey(s)
        if !was_relevant_for_hotkeys
            && !self.frontend_overlay_active
            && let Some(MachineContext { machine, .. }) = &self.machine_context
            && let Some(program_specification) = machine.program_specification()
        {
            // Enter runtime
            let runtime_guard = machine.enter_runtime();

            let program_specific_mappings = physical_gamepad_configuration
                .program_specific_mappings
                .entry(program_specification.id.clone())
                .or_default();

            for logical_input_device_path in &physical_input_device_state.controlling_input_devices
            {
                let logical_input_device_specific_mappings = program_specific_mappings
                    .entry(logical_input_device_path.clone())
                    .or_insert_with(|| {
                        let logical_input_device = runtime_guard
                            .input_devices()
                            .get(logical_input_device_path)
                            .unwrap();

                        logical_input_device
                            .metadata()
                            .default_mappings
                            .iter()
                            .map(|(from, to)| (*from, *to))
                            .collect()
                    });

                if let Some(transformed_input) =
                    logical_input_device_specific_mappings.get(&input_id)
                {
                    runtime_guard
                        .insert_inputs(logical_input_device_path, [(*transformed_input, state)]);
                }
            }
        }
    }

    pub fn register_gamepad(
        &mut self,
        id: PhysicalInputDeviceId,
        name: impl Into<Cow<'static, str>>,
        is_id_stable: bool,
        rely_on_frontend_input_handling: bool,
    ) {
        let name = name.into();

        tracing::info!("Gamepad with name {} attached ({})", name, id);

        // Make sure our main loop is ran
        self.egui_context.request_repaint();

        let mut controlling_input_devices = IndexSet::default();

        // HACK: Make sure this gamepad picks up a input device
        if let Some(MachineContext { machine, .. }) = &self.machine_context {
            let runtime_guard = machine.enter_runtime();

            if let Some(input_device_path) = runtime_guard.input_devices().keys().next() {
                controlling_input_devices.insert(input_device_path.clone());
            }
        }

        self.physical_input_devices.insert(
            id,
            PhysicalInputDeviceState {
                is_id_stable,
                name,
                rely_on_frontend_input_handling,
                gui_relevant_input_state: IndexMap::default(),
                controlling_input_devices,
            },
        );
    }

    pub fn unregister_gamepad(&mut self, id: PhysicalInputDeviceId) {
        self.physical_input_devices.remove(&id);
    }
}
