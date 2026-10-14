use std::borrow::Cow;

use fluxemu_input::{
    InputId, InputState,
    physical::{PhysicalInputDeviceId, hotkey::Hotkey},
};
use fluxemu_runtime::ResourcePath;
use indexmap::{IndexMap, IndexSet};

use super::{FrontendCore, FrontendEvent, MachineContext};
use crate::Platform;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputRouting {
    Handled,
    Ui,
}

#[derive(Debug, Clone)]
pub(super) struct PhysicalInputDeviceState {
    /// Should the runtime translate this input device into something the UI can understand
    pub rely_on_frontend_input_handling: bool,
    pub name: Cow<'static, str>,
    pub gui_relevant_input_state: IndexMap<InputId, InputState>,
    pub controlling_input_devices: IndexSet<ResourcePath>,
}

impl<P: Platform> FrontendCore<P> {
    pub fn insert_input(
        &mut self,
        origin: PhysicalInputDeviceId,
        input_id: InputId,
        state: InputState,
        machine_has_focus: bool,
    ) -> InputRouting {
        let Some(physical_input_device_state) = self.physical_input_devices.get_mut(&origin) else {
            tracing::error!("Ignoring unknown device {}", origin);

            return InputRouting::Handled;
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
                        self.events.push(FrontendEvent::MenuToggleRequested);
                    }
                    Hotkey::FastForward => {}
                    Hotkey::LoadSnapshot => {}
                    Hotkey::StoreSnapshot => {}
                    Hotkey::IncrementSnapshotCounter => {
                        self.environment.active_snapshot_slot += 1;

                        tracing::info!(
                            "Active snapshot slot is now {}",
                            self.environment.active_snapshot_slot
                        );
                    }
                    Hotkey::DecrementSnapshotCounter => {
                        self.environment.active_snapshot_slot -= 1;

                        tracing::info!(
                            "Current snapshot slot is now {}",
                            self.environment.active_snapshot_slot
                        );
                    }
                }
            }
        }

        let mut routing = InputRouting::Handled;

        // Ignore if that key participated in a hotkey(s)
        if !was_relevant_for_hotkeys {
            if machine_has_focus {
                if let Some(MachineContext {
                    machine,
                    controller,
                }) = &self.machine_context
                    && !controller.get_paused()
                    && let Some(program_specification) = machine.program_specification()
                {
                    // Enter runtime
                    let runtime_guard = machine.enter_runtime();

                    let program_specific_mappings = physical_gamepad_configuration
                        .program_specific_mappings
                        .entry(program_specification.id.clone())
                        .or_default();

                    for logical_input_device_path in
                        &physical_input_device_state.controlling_input_devices
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
                            runtime_guard.insert_inputs(
                                logical_input_device_path,
                                [(*transformed_input, state)],
                            );
                        }
                    }
                }
            } else if physical_input_device_state.rely_on_frontend_input_handling {
                routing = InputRouting::Ui;
            }
        }

        routing
    }

    pub fn register_gamepad(
        &mut self,
        id: PhysicalInputDeviceId,
        name: impl Into<Cow<'static, str>>,
        rely_on_frontend_input_handling: bool,
    ) {
        let name = name.into();

        tracing::info!("Gamepad with name {} attached ({})", name, id);

        let mut controlling_input_devices = IndexSet::default();

        // HACK: Make sure this gamepad picks up a input device
        if let Some(machine) = self.machine() {
            let runtime_guard = machine.enter_runtime();

            if let Some(input_device_path) = runtime_guard.input_devices().keys().next() {
                controlling_input_devices.insert(input_device_path.clone());
            }
        }

        self.physical_input_devices.insert(
            id,
            PhysicalInputDeviceState {
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
