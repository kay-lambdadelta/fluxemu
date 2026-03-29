//! Generic multi platform frontend implementation for fluxemu

mod backend;

mod file_browser;
mod input;
mod machine_factories;
mod machine_thread;
mod platform;
mod settings;

use std::{
    collections::HashMap,
    num::Wrapping,
    sync::{Arc, mpsc},
    thread::JoinHandle,
};

pub use backend::*;
use egui::{
    CentralPanel, Color32, ComboBox, Context, FontFamily, Frame, FullOutput, Id, Modal, Panel,
    RawInput, RichText, TextEdit, TextStyle,
};
use egui_extras::{Column, TableBuilder};
use egui_material_icons::{
    MaterialIcon,
    icons::{ICON_FOLDER, ICON_GAMEPAD, ICON_INFO, ICON_SETTINGS, ICON_VIDEO_LIBRARY},
};
use fluxemu_environment::{Environment, input::PhysicalGamepadConfiguration};
use fluxemu_input::{
    InputId, InputState,
    physical::{PhysicalInputDeviceId, hotkey::Hotkey},
};
use fluxemu_program::{MachineId, ProgramManager, ProgramSpecification, RomId};
use fluxemu_runtime::{
    graphics::GraphicsApi,
    machine::{
        Machine,
        builder::{MachineError, SealedMachineBuilder},
    },
    path::ResourcePath,
    persistence::SnapshotSlot,
    platform::Platform,
};
use indexmap::IndexMap;
pub use input::PhysicalInputDeviceMetadata;
pub use machine_factories::MachineFactories;
use palette::Srgba;
pub use platform::*;
use strum::{AsRefStr, EnumIter, IntoEnumIterator};

use crate::{
    file_browser::FileBrowserState,
    machine_thread::{MachineThreadContext, machine_thread},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, EnumIter, AsRefStr)]
pub enum TabId {
    Library,
    FileBrowser,
    Settings,
    Controller,
    About,
}

impl TabId {
    fn icon(self) -> MaterialIcon {
        match self {
            Self::FileBrowser => ICON_FOLDER,
            Self::Library => ICON_VIDEO_LIBRARY,
            Self::Settings => ICON_SETTINGS,
            Self::Controller => ICON_GAMEPAD,
            Self::About => ICON_INFO,
        }
    }
}

struct MachineContext {
    offload_communication: mpsc::Sender<machine_thread::Message>,
    offload_handle: std::thread::JoinHandle<()>,
    machine: Arc<Machine>,
    physical_input_to_virtual_mapping: HashMap<PhysicalInputDeviceId, ResourcePath>,
}

#[derive(Debug, Clone)]
struct PhysicalInputDeviceState {
    pub is_id_stable: bool,
    pub metadata: PhysicalInputDeviceMetadata,
    // Should the runtime translate this input device into something egui can understand
    pub feed_into_gui: bool,
    pub gui_relevant_input_state: IndexMap<InputId, InputState>,
}

enum MachineInitializationStep<P: Platform> {
    /// Step 1: Compute ROM ids for recognition
    CalculatingRomIds {
        job: JoinHandle<Result<Vec<RomId>, fluxemu_program::Error>>,
    },
    /// Step 2: Search for programs that match the collection of given ROMs
    FindingMatchingSpecification {
        roms: Vec<RomId>,
        job: JoinHandle<Result<Vec<ProgramSpecification>, fluxemu_program::Error>>,
    },
    /// Step 3: Create and seal a machine builder given the specification
    BuildingMachineBuilder {
        job: JoinHandle<Result<SealedMachineBuilder<P>, MachineError>>,
    },
}

/// Frontend for the emulator
#[allow(clippy::type_complexity)]
pub struct Frontend<P: FrontendPlatform> {
    pub environment: Environment,
    machine_context: Option<MachineContext>,
    pending_machine: Option<SealedMachineBuilder<P>>,
    audio_runtime: P::AudioRuntime,
    current_snapshot_slot: Wrapping<SnapshotSlot>,
    machine_factories: Arc<MachineFactories<P>>,
    program_manager: Arc<ProgramManager>,
    machine_loading: bool,
    frontend_overlay_active: bool,
    current_tab: TabId,
    physical_input_devices: HashMap<PhysicalInputDeviceId, PhysicalInputDeviceState>,
    egui_context: Context,
    file_browser: FileBrowserState,
    machine_initialization_step: Option<MachineInitializationStep<P>>,

    #[cfg(feature = "external-file-dialog")]
    native_file_picker_dialog_job: Option<JoinHandle<Option<rfd::FileHandle>>>,
}

impl<P: FrontendPlatform> Frontend<P> {
    pub fn new(
        environment: Environment,
        machine_factories: MachineFactories<P>,
        program_manager: Arc<ProgramManager>,
        audio_runtime: P::AudioRuntime,
        initial_program: Option<Vec<RomId>>,
    ) -> Self {
        let initial_program_initialization_step = initial_program.map(|roms| {
            let program_manager = program_manager.clone();

            MachineInitializationStep::FindingMatchingSpecification {
                roms: roms.clone(),
                job: std::thread::spawn(move || program_manager.identify_program(&roms)),
            }
        });

        Self {
            file_browser: FileBrowserState::new(environment.file_browser_home_directory.clone()),
            machine_context: None,
            pending_machine: None,
            environment,
            audio_runtime,
            machine_factories: Arc::new(machine_factories),
            current_snapshot_slot: Wrapping(SnapshotSlot::default()),
            program_manager,
            machine_loading: false,
            frontend_overlay_active: true,
            current_tab: TabId::Library,
            physical_input_devices: HashMap::default(),
            egui_context: setup_egui_context(),
            #[cfg(feature = "external-file-dialog")]
            native_file_picker_dialog_job: None,
            machine_initialization_step: initial_program_initialization_step,
        }
    }

    pub fn egui_context(&self) -> &egui::Context {
        &self.egui_context
    }

    pub fn machine(&self) -> Option<&Arc<Machine>> {
        self.machine_context
            .as_ref()
            .map(|context| &context.machine)
    }

    pub fn frontend_overlay_active(&self) -> bool {
        self.frontend_overlay_active
    }

    fn bring_down_current_machine(&mut self) {
        if let Some(MachineContext {
            offload_communication,
            offload_handle,
            machine,
            physical_input_to_virtual_mapping: _,
        }) = self.machine_context.take()
        {
            // Hang up
            drop(offload_communication);

            // Wait for the thread to exit
            offload_handle.join().unwrap();

            // Destroy old machine
            drop(machine);
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

    pub fn change_input_state(
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
                                offload_communication,
                                ..
                            }) = &mut self.machine_context
                            {
                                // Unpause emulation
                                offload_communication
                                    .send(machine_thread::Message::Pause(false))
                                    .unwrap();

                                // We don't allow the overlay to be deactivated if there isn't an active machine
                                self.frontend_overlay_active = false;
                            }
                        } else {
                            // Pause machine if one is active
                            if let Some(MachineContext {
                                offload_communication,
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
                                offload_communication
                                    .send(machine_thread::Message::Pause(true))
                                    .unwrap();
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

    fn build_machine_for_specification(&mut self, specification: ProgramSpecification) {
        let program_manager = self.program_manager.clone();
        let save_path = self.environment.save_directory.clone();
        let snapshot_path = self.environment.snapshot_directory.clone();
        let machine_factories = self.machine_factories.clone();

        let machine_builder = Machine::build(
            Some(specification),
            program_manager,
            Some(save_path),
            Some(snapshot_path),
        );

        let handle = std::thread::spawn(move || {
            machine_factories
                .construct_machine(machine_builder)
                .unwrap()
                .seal()
        });

        self.machine_initialization_step =
            Some(MachineInitializationStep::BuildingMachineBuilder { job: handle });
    }

    pub fn reset_graphics_to_meet_machine_requirements(
        &mut self,
        callback: impl FnOnce(
            &Context,
            &SealedMachineBuilder<P>,
        ) -> <P::GraphicsApi as GraphicsApi>::InitializationData,
    ) {
        if let Some(sealed_machine_builder) = self.pending_machine.take() {
            self.egui_context = setup_egui_context();

            // NOTE: This will block the ui
            self.bring_down_current_machine();

            let graphics_initialization_data =
                callback(&self.egui_context, &sealed_machine_builder);

            let machine = sealed_machine_builder.build(graphics_initialization_data);
            let runtime_guard = machine.enter_runtime();

            let (offload_communication_sender, offload_communication_receiver) = mpsc::channel();

            let offload_handle = std::thread::Builder::new()
                .name("machine-simulation".into())
                .spawn({
                    let machine = machine.clone();

                    || {
                        machine_thread(MachineThreadContext {
                            message_receiver: offload_communication_receiver,
                            machine,
                        });
                    }
                })
                .expect("Spawning offloading thread failed");

            // FIXME: Actually reference the environment and add a input mapping ui
            let default_physical_input_to_virtual_mapping =
                if let Some(input_device) = runtime_guard.input_devices().keys().next() {
                    HashMap::from([(
                        PhysicalInputDeviceId::PLATFORM_RESERVED,
                        input_device.clone(),
                    )])
                } else {
                    HashMap::default()
                };

            // Exit runtime
            drop(runtime_guard);

            self.machine_context = Some(MachineContext {
                offload_communication: offload_communication_sender,
                offload_handle,
                machine,
                physical_input_to_virtual_mapping: default_physical_input_to_virtual_mapping,
            });

            self.machine_loading = false;
            self.frontend_overlay_active = false;
        }
    }

    pub fn run_menu(&mut self, external_input: RawInput) -> FullOutput {
        self.egui_context.clone().run_ui(external_input, |ctx| {
            if let Some(machine_initialization_step) = self.machine_initialization_step.take() {
                self.service_machine_initialization_step(machine_initialization_step);
            }

            Panel::top("menu_selection")
                .resizable(false)
                .min_size(50.0)
                .show_inside(ctx, |ui| {
                    ui.horizontal(|ui| {
                        for tab in TabId::iter() {
                            let mut item_icon = RichText::new(tab.icon()).size(32.0);

                            if self.current_tab == tab {
                                item_icon = item_icon.strong();
                            }

                            if ui.button(item_icon).on_hover_text(tab.as_ref()).clicked() {
                                self.current_tab = tab;
                            }
                        }
                    });
                });

            CentralPanel::default().show_inside(ctx, |ui| {
                ui.with_layout(egui::Layout::top_down_justified(egui::Align::LEFT), |ui| {
                    Frame::new()
                        .inner_margin(10.0)
                        .show(ui, |ui| match self.current_tab {
                            TabId::Library => {}
                            TabId::FileBrowser => {
                                self.handle_file_browser(ui);
                            }
                            TabId::Settings => {
                                self.handle_settings(ui);
                            }
                            TabId::Controller => {}
                            TabId::About => {}
                        });
                });
            });
        })
    }

    fn service_machine_initialization_step(&mut self, step: MachineInitializationStep<P>) {
        match step {
            MachineInitializationStep::CalculatingRomIds { job } if job.is_finished() => {
                match job.join().unwrap() {
                    Ok(roms) => {
                        let program_manager = self.program_manager.clone();

                        self.machine_initialization_step =
                            Some(MachineInitializationStep::FindingMatchingSpecification {
                                roms: roms.clone(),
                                job: std::thread::spawn(move || {
                                    program_manager.identify_program(&roms)
                                }),
                            });
                    }
                    Err(err) => tracing::error!("Failed to calculate ROM ids: {}", err),
                }
            }
            MachineInitializationStep::FindingMatchingSpecification { job, roms }
                if job.is_finished() =>
            {
                match job.join().unwrap() {
                    Ok(mut specifications) => {
                        let specification = if !specifications.is_empty() {
                            specifications.remove(0)
                        } else {
                            let Ok(Some(program_specification)) =
                                self.program_manager.auto_generate_specification(roms[0])
                            else {
                                tracing::error!("Could not properly identify program");

                                return;
                            };

                            program_specification
                        };

                        self.build_machine_for_specification(specification);
                    }
                    Err(err) => tracing::error!("Failed to find matching specification: {}", err),
                }
            }
            MachineInitializationStep::BuildingMachineBuilder { job } if job.is_finished() => {
                match job.join().unwrap() {
                    Ok(sealed) => self.pending_machine = Some(sealed),
                    Err(err) => tracing::error!("Failed to build machine_builder: {}", err),
                }
            }
            unfinished => self.machine_initialization_step = Some(unfinished),
        }
    }
}

fn specification_fillout_clarification_modal(
    ctx: &Context,
    specification: &mut ProgramSpecification,
) {
    let modal = Modal::new(Id::new("specification-fillout-clarification-modal"));

    let response = modal.show(ctx, |ui| {
        ui.heading("Please fill in info you know about this program");
        ui.label("This program needs information on it to execute it");
        ui.separator();

        TableBuilder::new(ui)
            .column(Column::auto().resizable(true))
            .column(Column::remainder())
            .striped(true)
            .body(|mut body| {
                body.row(30.0, |mut row| {
                    row.col(|ui| {
                        ui.label("Machine ID");
                    });
                    row.col(|ui| {
                        ComboBox::from_id_salt("Machine ID")
                            .selected_text(specification.id.machine.to_nointro_string())
                            .show_ui(ui, |ui| {
                                for machine_id in MachineId::iter() {
                                    let no_intro_string = machine_id.to_nointro_string();

                                    ui.selectable_value(
                                        &mut specification.id.machine,
                                        machine_id,
                                        no_intro_string,
                                    );
                                }
                            });
                    });
                });

                body.row(30.0, |mut row| {
                    row.col(|ui| {
                        ui.label("Program Name");
                    });
                    row.col(|ui| {
                        TextEdit::singleline(&mut specification.id.name).show(ui);
                    });
                });
            });
    });

    if response.should_close() {}
}

fn setup_egui_context() -> Context {
    let egui_context = Context::default();
    egui_material_icons::initialize(&egui_context);

    egui_context.global_style_mut(|style| {
        style.text_styles.insert(
            TextStyle::Body,
            egui::FontId::new(18.0, FontFamily::Proportional),
        );
        style.text_styles.insert(
            TextStyle::Button,
            egui::FontId::new(20.0, FontFamily::Proportional),
        );
        style.text_styles.insert(
            TextStyle::Heading,
            egui::FontId::new(24.0, FontFamily::Proportional),
        );
    });

    egui_context
}

fn to_egui_color(color: impl Into<Srgba<u8>>) -> Color32 {
    let color = color.into();

    Color32::from_rgba_unmultiplied(color.red, color.green, color.blue, color.alpha)
}
