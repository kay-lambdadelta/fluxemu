use std::borrow::Cow;

use fluxemu_environment::input::InputMapping;
use fluxemu_input::{
    DirectionPart, InputId, InputShape, InputState,
    host::{HostInputDeviceInstanceId, HostInputDeviceKind, hotkey::Hotkey},
};
use fluxemu_runtime::{
    ResourcePath,
    input::{ErasedGuestInputDevice, ResolvedGuestInput},
};
use indexmap::{IndexMap, IndexSet};
use nalgebra::Vector2;

use super::{FrontendCore, FrontendEvent, MachineContext};
use crate::Platform;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputRouting {
    Handled,
    Ui,
}

#[derive(Debug, Clone)]
pub(super) struct HostInputDeviceState {
    /// Should the runtime translate this input device into something the UI can understand
    pub rely_on_frontend_input_handling: bool,
    pub name: Cow<'static, str>,
    pub kind: HostInputDeviceKind,
    pub gui_relevant_input_state: IndexMap<InputId, InputState>,
    pub controlling_input_devices: IndexSet<ResourcePath>,
}

impl<P: Platform> FrontendCore<P> {
    pub fn insert_input(
        &mut self,
        origin: HostInputDeviceInstanceId,
        input_id: InputId,
        state: InputState,
        machine_has_focus: bool,
    ) -> InputRouting {
        if input_id.split().1 != DirectionPart::All || input_id.shape() != state.shape() {
            tracing::error!(
                "Ignoring {:?} for {:?}, which is not a whole input of that shape",
                state,
                input_id
            );

            return InputRouting::Handled;
        }

        let mut deferred_hotkey = None;

        let routing = self.route_input(
            origin,
            input_id,
            state,
            machine_has_focus,
            &mut deferred_hotkey,
        );

        match deferred_hotkey {
            Some(Hotkey::LoadSnapshot) => self.load_snapshot(self.active_snapshot_slot()),
            Some(Hotkey::StoreSnapshot) => {
                self.pending_snapshot_stores
                    .push(self.active_snapshot_slot());
            }
            Some(Hotkey::UndoLoadSnapshot) => self.undo_load_snapshot(),
            Some(Hotkey::UndoStoreSnapshot) => self.undo_store_snapshot(),
            _ => {}
        }

        routing
    }

    fn route_input(
        &mut self,
        origin: HostInputDeviceInstanceId,
        input_id: InputId,
        state: InputState,
        machine_has_focus: bool,
        deferred_hotkey: &mut Option<Hotkey>,
    ) -> InputRouting {
        let Some(host_input_device_state) = self.host_input_devices.get_mut(&origin) else {
            tracing::error!("Ignoring unknown device {}", origin);

            return InputRouting::Handled;
        };

        tracing::trace!(
            ?origin,
            ?input_id,
            ?state,
            "{}",
            host_input_device_state.name
        );

        let host_gamepad_configuration =
            self.environment.gamepads.entry(origin.device).or_default();

        let hotkeys_activated_before: Vec<_> = host_gamepad_configuration
            .hotkey
            .keys()
            .map(|combination| {
                is_combination_held(
                    &host_input_device_state.gui_relevant_input_state,
                    combination,
                )
            })
            .collect();

        host_input_device_state
            .gui_relevant_input_state
            .insert(input_id, state);

        let mut was_relevant_for_hotkeys = false;

        // Check for hotkeys
        for ((combination, hotkey_action), was_activated) in host_gamepad_configuration
            .hotkey
            .iter()
            .zip(hotkeys_activated_before)
        {
            if !combination
                .iter()
                .any(|member| member.split().0 == input_id)
            {
                continue;
            }

            let is_activated = is_combination_held(
                &host_input_device_state.gui_relevant_input_state,
                combination,
            );

            if is_activated {
                was_relevant_for_hotkeys = true;

                if !was_activated {
                    match hotkey_action {
                        Hotkey::ToggleMenu => {
                            self.events.push(FrontendEvent::MenuToggleRequested);
                        }
                        Hotkey::ToggleDebugOverlay => {
                            self.events.push(FrontendEvent::DebugOverlayToggleRequested);
                        }
                        Hotkey::FastForward => {}
                        Hotkey::LoadSnapshot
                        | Hotkey::StoreSnapshot
                        | Hotkey::UndoLoadSnapshot
                        | Hotkey::UndoStoreSnapshot => {
                            *deferred_hotkey = Some(*hotkey_action);
                        }
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
                    ..
                }) = &self.machine_context
                    && !controller.get_paused()
                    && let Some(program_specification) = machine.program_specification()
                {
                    // Enter runtime
                    let runtime_guard = machine.enter_runtime();

                    let program_specific_mappings = host_gamepad_configuration
                        .program_specific_mappings
                        .entry(program_specification.id.clone())
                        .or_default();

                    for guest_input_device_path in
                        &host_input_device_state.controlling_input_devices
                    {
                        let guest_input_device = runtime_guard
                            .input_devices()
                            .get(guest_input_device_path)
                            .unwrap();

                        let guest_input_device_specific_mappings = program_specific_mappings
                            .entry(guest_input_device_path.clone())
                            .or_insert_with(|| {
                                guest_input_device
                                    .metadata()
                                    .default_mappings
                                    .iter()
                                    .map(|(from, to)| (*from, to.clone()))
                                    .collect()
                            });

                        let host_input_state = &host_input_device_state.gui_relevant_input_state;

                        let mut affected_inputs: Vec<_> = Vec::new();

                        for (host_input, guest_input) in guest_input_device_specific_mappings.iter()
                        {
                            if host_input.split().0 != input_id {
                                continue;
                            }

                            let Some(resolved) = guest_input_device.resolve(guest_input) else {
                                continue;
                            };

                            if !affected_inputs.iter().any(|affected: &ResolvedGuestInput| {
                                affected.whole == resolved.whole
                            }) {
                                affected_inputs.push(resolved);
                            }
                        }

                        let combined_inputs: Vec<_> = affected_inputs
                            .into_iter()
                            .map(|affected| {
                                let value = combine_bound_inputs(
                                    &**guest_input_device,
                                    guest_input_device_specific_mappings,
                                    host_input_state,
                                    &affected,
                                );

                                (affected.whole, value)
                            })
                            .collect();

                        runtime_guard.insert_inputs(
                            guest_input_device_path,
                            combined_inputs
                                .iter()
                                .map(|(whole, value)| (&**whole, *value)),
                        );
                    }
                }
            } else if host_input_device_state.rely_on_frontend_input_handling {
                routing = InputRouting::Ui;
            }
        }

        routing
    }

    pub fn register_gamepad(
        &mut self,
        id: HostInputDeviceInstanceId,
        name: impl Into<Cow<'static, str>>,
        kind: HostInputDeviceKind,
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

        self.host_input_devices.insert(
            id,
            HostInputDeviceState {
                name,
                kind,
                rely_on_frontend_input_handling,
                gui_relevant_input_state: IndexMap::default(),
                controlling_input_devices,
            },
        );
    }

    pub fn unregister_gamepad(&mut self, id: HostInputDeviceInstanceId) {
        let Some(host_input_device_state) = self.host_input_devices.remove(&id) else {
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

        for guest_input_device_path in &host_input_device_state.controlling_input_devices {
            let Some(mappings) = program_specific_mappings.get(guest_input_device_path) else {
                continue;
            };

            let Some(guest_input_device) =
                runtime_guard.input_devices().get(guest_input_device_path)
            else {
                continue;
            };

            let released_inputs: Vec<_> = mappings
                .iter()
                .filter(|(host_input, _)| {
                    host_input_device_state
                        .gui_relevant_input_state
                        .get(&host_input.split().0)
                        .is_some_and(|value| *value != value.shape().neutral())
                })
                .filter_map(|(_, guest_input)| guest_input_device.resolve(guest_input))
                .map(|resolved| (resolved.whole, resolved.shape.neutral()))
                .collect();

            runtime_guard.insert_inputs(
                guest_input_device_path,
                released_inputs
                    .iter()
                    .map(|(whole, value)| (&**whole, *value)),
            );
        }
    }

    pub fn host_input_devices(
        &self,
    ) -> impl Iterator<Item = (HostInputDeviceInstanceId, &str, HostInputDeviceKind)> {
        self.host_input_devices
            .iter()
            .map(|(id, state)| (*id, state.name.as_ref(), state.kind))
    }
}

fn host_part_value(state: &IndexMap<InputId, InputState>, input: InputId) -> Option<InputState> {
    let (whole, part) = input.split();

    state
        .get(&whole)
        .copied()
        .unwrap_or_else(|| whole.shape().neutral())
        .part(part)
}

fn is_combination_held<'a>(
    state: &IndexMap<InputId, InputState>,
    combination: impl IntoIterator<Item = &'a InputId>,
) -> bool {
    combination
        .into_iter()
        .all(|member| host_part_value(state, *member).is_some_and(|value| value.is_held()))
}

fn combine_bound_inputs(
    guest_input_device: &dyn ErasedGuestInputDevice,
    mappings: &InputMapping,
    host_input_state: &IndexMap<InputId, InputState>,
    target: &ResolvedGuestInput,
) -> InputState {
    let mut button = 0.0f32;
    let mut direction = Vector2::zeros();

    for (host_input, guest_input) in mappings {
        let Some(resolved) = guest_input_device.resolve(guest_input) else {
            continue;
        };

        if resolved.whole != target.whole {
            continue;
        }

        let Some(host_value) = host_part_value(host_input_state, *host_input) else {
            continue;
        };

        match (target.shape, resolved.part.unit_vector(), host_value) {
            (InputShape::Button, _, InputState::Button(value)) => button = button.max(value),
            (InputShape::Direction, None, InputState::Direction(value)) => direction += value,
            (InputShape::Direction, Some(cardinal), InputState::Button(value)) => {
                direction += cardinal * value
            }
            _ => tracing::warn!(
                "Binding {:?} to {} has mismatched shapes",
                host_input,
                guest_input
            ),
        }
    }

    match target.shape {
        InputShape::Button => InputState::button(button),
        InputShape::Direction => InputState::direction(direction),
    }
}
