use std::borrow::Cow;

use fluxemu_input::{
    InputId, InputState,
    physical::{PhysicalInputDeviceInstanceId, PhysicalInputDeviceKind, hotkey::Hotkey},
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
    pub kind: PhysicalInputDeviceKind,
    pub gui_relevant_input_state: IndexMap<InputId, InputState>,
    pub controlling_input_devices: IndexSet<ResourcePath>,
}

impl<P: Platform> FrontendCore<P> {
    pub fn insert_input(
        &mut self,
        origin: PhysicalInputDeviceInstanceId,
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

        let physical_gamepad_configuration =
            self.environment.gamepads.entry(origin.device).or_default();

        let was_pressed = physical_input_device_state
            .gui_relevant_input_state
            .insert(input_id, state)
            .is_some_and(|previous_state| previous_state.as_digital(None));
        let is_press_edge = !was_pressed && state.as_digital(None);

        let mut was_relevant_for_hotkeys = false;

        // Check for hotkeys
        for (combinations, hotkey_action) in &physical_gamepad_configuration.hotkey {
            if !combinations.contains(&input_id) {
                continue;
            }

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

                if is_press_edge {
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
                            let is_present = runtime_guard
                                .input_devices()
                                .get(logical_input_device_path)
                                .unwrap()
                                .metadata()
                                .present_inputs
                                .contains(transformed_input);

                            if is_present {
                                runtime_guard.insert_inputs(
                                    logical_input_device_path,
                                    [(*transformed_input, state)],
                                );
                            } else {
                                tracing::warn!(
                                    "Ignoring mapping to {:?}, which {} does not have",
                                    transformed_input,
                                    logical_input_device_path
                                );
                            }
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
        id: PhysicalInputDeviceInstanceId,
        name: impl Into<Cow<'static, str>>,
        kind: PhysicalInputDeviceKind,
        rely_on_frontend_input_handling: bool,
    ) {
        let name = name.into();

        tracing::info!("Gamepad with name {} attached ({})", name, id);

        let mut controlling_input_devices = IndexSet::default();

        // HACK: Make sure this gamepad picks up a input device
        if let Some(machine) = self.machine() {
            let runtime_guard = machine.enter_runtime();

            if let Some(input_device_path) = runtime_guard.input_devices().keys().min() {
                controlling_input_devices.insert(input_device_path.clone());
            }
        }

        self.physical_input_devices.insert(
            id,
            PhysicalInputDeviceState {
                name,
                kind,
                rely_on_frontend_input_handling,
                gui_relevant_input_state: IndexMap::default(),
                controlling_input_devices,
            },
        );
    }

    pub fn unregister_gamepad(&mut self, id: PhysicalInputDeviceInstanceId) {
        let Some(physical_input_device_state) = self.physical_input_devices.remove(&id) else {
            return;
        };

        let Some(MachineContext { machine, .. }) = &self.machine_context else {
            return;
        };

        let Some(program_specific_mappings) =
            machine.program_specification().and_then(|specification| {
                self.environment
                    .gamepads
                    .get(&id.device)?
                    .program_specific_mappings
                    .get(&specification.id)
            })
        else {
            return;
        };

        let runtime_guard = machine.enter_runtime();

        for logical_input_device_path in &physical_input_device_state.controlling_input_devices {
            let Some(mappings) = program_specific_mappings.get(logical_input_device_path) else {
                continue;
            };

            let logical_input_device = &runtime_guard.input_devices()[logical_input_device_path];

            let released_inputs = physical_input_device_state
                .gui_relevant_input_state
                .iter()
                .filter(|(_, state)| state.as_analog() > 0.0)
                .filter_map(|(input_id, _)| mappings.get(input_id))
                .filter(|input_id| {
                    logical_input_device
                        .metadata()
                        .present_inputs
                        .contains(*input_id)
                })
                .map(|input_id| (*input_id, InputState::RELEASED));

            runtime_guard.insert_inputs(logical_input_device_path, released_inputs);
        }
    }

    pub fn physical_input_devices(
        &self,
    ) -> impl Iterator<Item = (PhysicalInputDeviceInstanceId, &str, PhysicalInputDeviceKind)> {
        self.physical_input_devices
            .iter()
            .map(|(id, state)| (*id, state.name.as_ref(), state.kind))
    }
}
