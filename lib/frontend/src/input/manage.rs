use fluxemu_environment::input::PhysicalGamepadConfiguration;
use fluxemu_input::{
    InputId, InputState,
    physical::{PhysicalInputDeviceId, hotkey::Hotkey},
};
use indexmap::IndexMap;

use crate::{
    Frontend, FrontendPlatform, MachineContext, PhysicalInputDeviceMetadata,
    PhysicalInputDeviceState,
};

impl<P: FrontendPlatform> Frontend<P> {
    pub fn insert_input(
        &mut self,
        origin: PhysicalInputDeviceId,
        input_id: InputId,
        state: InputState,
    ) {
        let Some(device) = self.physical_input_devices.get_mut(&origin) else {
            tracing::warn!("Ignoring unknown device {}", origin);

            return;
        };

        if !device.metadata.present_inputs.contains(&input_id) {
            tracing::warn!(
                "Ignoring unknown input {:?} from device {} in state {:?}",
                input_id,
                origin,
                state
            );

            return;
        }

        device.gui_relevant_input_state.insert(input_id, state);
        let mut was_relevant_for_hotkeys = false;

        // Check for hotkeys
        for (combinations, action) in self.environment.hotkeys.iter() {
            let is_activated = combinations.iter().all(|input_id| {
                device
                    .gui_relevant_input_state
                    .get(input_id)
                    .copied()
                    .unwrap_or(InputState::RELEASED)
                    .as_digital(None)
            });

            if is_activated {
                was_relevant_for_hotkeys = true;

                match action {
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
                        self.current_snapshot_slot += 1;
                    }
                    Hotkey::DecrementSnapshotCounter => {
                        self.current_snapshot_slot -= 1;
                    }
                }
            }
        }

        // Ignore if that key participated in a hotkey(s)
        if !was_relevant_for_hotkeys
            && !self.frontend_overlay_active
            && let Some(MachineContext {
                machine,
                physical_input_to_virtual_mapping,
                ..
            }) = &self.machine_context
        {
            // Enter runtime
            let runtime_guard = machine.enter_runtime();

            let Some(program_specification) = runtime_guard.program_specification() else {
                return;
            };

            let Some(input_path) = physical_input_to_virtual_mapping.get(&origin) else {
                return;
            };

            let Some(logical_device) = runtime_guard.input_devices().get(input_path) else {
                return;
            };

            let transformed = self
                .environment
                .physical_input_configs
                .get(&origin)
                .and_then(|physical_gamepad_configuration| {
                    let PhysicalGamepadConfiguration {
                        program_overrides, ..
                    } = physical_gamepad_configuration;
                    program_overrides
                        .get(&program_specification.id)
                        .and_then(|mapping| mapping.get(input_path))
                        .and_then(|mapping| mapping.get(&input_id).copied())
                })
                .or_else(|| {
                    logical_device
                        .metadata()
                        .default_mappings
                        .get(&input_id)
                        .copied()
                });

            let Some(transformed_input_id) = transformed else {
                return;
            };

            if !logical_device
                .metadata()
                .present_inputs
                .contains(&transformed_input_id)
            {
                tracing::error!(
                    "Transformed input targets unknown emulated input: {:?} on {:?}",
                    transformed_input_id,
                    logical_device
                );
                return;
            }

            if logical_device.get_state(transformed_input_id) == state {
                return;
            }

            logical_device.set_state(transformed_input_id, state);

            // Insert that input into the machine
            runtime_guard.insert_inputs(input_path, [(transformed_input_id, state)]);
        }
    }

    pub fn add_input_device(
        &mut self,
        id: PhysicalInputDeviceId,
        metadata: PhysicalInputDeviceMetadata,
        is_id_stable: bool,
        feed_into_gui: bool,
    ) {
        self.physical_input_devices.insert(
            id,
            PhysicalInputDeviceState {
                is_id_stable,
                metadata,
                feed_into_gui,
                gui_relevant_input_state: IndexMap::default(),
            },
        );
    }

    pub fn remove_input_device(&mut self, id: PhysicalInputDeviceId) {
        self.physical_input_devices.remove(&id);
    }
}
