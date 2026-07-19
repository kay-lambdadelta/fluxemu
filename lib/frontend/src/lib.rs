//! Generic multi platform frontend implementation for fluxemu

pub mod audio;
pub mod graphics;

mod file_browser;
mod input;
pub mod machine;
mod platform;
mod settings;
mod toast;

use std::{borrow::Cow, collections::HashMap, num::Wrapping, sync::Arc, thread::JoinHandle};

use egui::{
    Align, CentralPanel, Color32, Context, FontFamily, Frame, FullOutput, Layout, Panel, RawInput,
    RichText, TextStyle,
};
use egui_material_icons::{
    MaterialIcon,
    icons::{
        ICON_ARTICLE, ICON_BUG_REPORT, ICON_FOLDER, ICON_GAMEPAD, ICON_INFO, ICON_SETTINGS,
        ICON_VIDEO_LIBRARY,
    },
};
use egui_toast::ToastKind;
use fluxemu_environment::{ENVIRONMENT_LOCATION, Environment};
use fluxemu_graphics::api::GraphicsApi;
use fluxemu_input::{InputId, InputState, physical::PhysicalInputDeviceId};
use fluxemu_program::{ProgramManager, ProgramSpecification, RomId};
use fluxemu_runtime::{
    ResourcePath,
    machine::{Machine, builder::SealedMachineBuilder},
    platform::Platform,
};
use indexmap::{IndexMap, IndexSet};
use palette::Srgba;
pub use platform::*;
use strum::{AsRefStr, EnumIter, IntoEnumIterator};

use crate::{
    audio::{AudioRuntime, mixer::AudioMixer},
    file_browser::{FileBrowser, state::FileBrowserState},
    machine::{FactoryManager, SimulationController},
    toast::ToastManager,
};

rust_i18n::i18n!("locales", fallback = "en");

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, EnumIter, AsRefStr)]
pub enum TabId {
    Library,
    FileBrowser,
    Settings,
    Log,
    Controller,
    Debug,
    About,
}

impl TabId {
    fn icon(self) -> MaterialIcon {
        match self {
            Self::FileBrowser => ICON_FOLDER,
            Self::Library => ICON_VIDEO_LIBRARY,
            Self::Settings => ICON_SETTINGS,
            Self::Log => ICON_ARTICLE,
            Self::Controller => ICON_GAMEPAD,
            Self::Debug => ICON_BUG_REPORT,
            Self::About => ICON_INFO,
        }
    }
}

struct MachineContext {
    machine: Arc<Machine>,
    simulation_controller: SimulationController,
}

#[derive(Debug, Clone)]
struct PhysicalInputDeviceState {
    pub is_id_stable: bool,
    // Should the runtime translate this input device into something egui can understand
    pub rely_on_frontend_input_handling: bool,
    pub name: Cow<'static, str>,
    pub gui_relevant_input_state: IndexMap<InputId, InputState>,
    pub controlling_input_devices: IndexSet<ResourcePath>,
}

#[derive(Debug)]
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
        job: JoinHandle<Option<SealedMachineBuilder<P>>>,
    },
}

/// Frontend for the emulator
#[allow(clippy::type_complexity)]
pub struct Frontend<P: FrontendPlatform> {
    pub environment: Environment,
    machine_context: Option<MachineContext>,
    pending_machine: Option<SealedMachineBuilder<P>>,
    audio_runtime: P::AudioRuntime,
    current_snapshot_slot: Wrapping<u8>,
    machine_factory_manager: Arc<FactoryManager<P>>,
    program_manager: Arc<ProgramManager>,
    machine_loading: bool,
    frontend_overlay_active: bool,
    current_tab: TabId,
    physical_input_devices: HashMap<PhysicalInputDeviceId, PhysicalInputDeviceState>,
    egui_context: Context,
    file_browser_state: FileBrowserState,
    machine_initialization_step: Option<MachineInitializationStep<P>>,
    toast_manager: ToastManager,
    audio_mixer: Arc<AudioMixer>,
    external_file_dialog_supported: bool,
}

impl<P: FrontendPlatform> Frontend<P> {
    pub fn new(
        environment: Environment,
        machine_factories: FactoryManager<P>,
        program_manager: Arc<ProgramManager>,
        mut audio_runtime: P::AudioRuntime,
        initial_program: Option<Vec<RomId>>,
        external_file_dialog_supported: bool,
    ) -> Self {
        let initial_program_initialization_step = initial_program.map(|roms| {
            let program_manager = program_manager.clone();

            MachineInitializationStep::FindingMatchingSpecification {
                roms: roms.clone(),
                job: std::thread::spawn(move || program_manager.identify_program(&roms)),
            }
        });

        let sample_rate = audio_runtime.sample_rate();
        let audio_mixer = Arc::new(AudioMixer::new(sample_rate));
        audio_runtime.set_audio_mixer(audio_mixer.clone());

        Self {
            machine_context: None,
            pending_machine: None,
            audio_runtime,
            machine_factory_manager: Arc::new(machine_factories),
            current_snapshot_slot: Wrapping(0),
            program_manager,
            machine_loading: false,
            frontend_overlay_active: true,
            current_tab: TabId::Library,
            physical_input_devices: HashMap::default(),
            egui_context: setup_egui_context(),
            audio_mixer,
            file_browser_state: FileBrowserState::new(
                environment.file_browser_home_directory.clone(),
            ),
            toast_manager: ToastManager::default(),
            machine_initialization_step: initial_program_initialization_step,
            environment,
            external_file_dialog_supported,
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

    pub fn overlay_active(&self) -> bool {
        self.frontend_overlay_active
    }

    fn bring_down_current_machine(&mut self) {
        self.machine_context = None;

        for physical_gamepad_state in self.physical_input_devices.values_mut() {
            physical_gamepad_state.controlling_input_devices.clear();
        }
    }

    fn build_machine_for_specification(&mut self, specification: ProgramSpecification) {
        let program_manager = self.program_manager.clone();
        let machine_factories = self.machine_factory_manager.clone();

        let machine_builder = Machine::build(Some(specification), program_manager);

        let handle = std::thread::spawn(move || {
            Some(machine_factories.construct_machine(machine_builder)?.seal())
        });

        self.machine_initialization_step =
            Some(MachineInitializationStep::BuildingMachineBuilder { job: handle });
    }

    pub fn maybe_reset_graphics_to_meet_machine_requirements(
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

            // HACK: Assign the first input device to all gamepads
            if let Some(logical_input_device_path) = runtime_guard.input_devices().keys().next() {
                for physical_input_device_state in self.physical_input_devices.values_mut() {
                    physical_input_device_state
                        .controlling_input_devices
                        .insert(logical_input_device_path.clone());
                }
            }

            // Exit runtime
            drop(runtime_guard);

            let simulation_controller =
                SimulationController::new(machine.clone(), self.audio_mixer.clone());

            // Make sure the simulation is currently running
            simulation_controller.set_paused(false);

            self.machine_context = Some(MachineContext {
                simulation_controller,
                machine,
            });

            self.machine_loading = false;
            self.frontend_overlay_active = false;
        }
    }

    pub fn run_menu(&mut self, external_input: RawInput) -> FullOutput {
        self.egui_context.clone().run_ui(external_input, |ui| {
            if let Some(machine_initialization_step) = self.machine_initialization_step.take() {
                self.service_machine_initialization_step(machine_initialization_step);
            }

            self.toast_manager.show(ui);

            Panel::top("menu_selection")
                .resizable(false)
                .show(ui, |ui| {
                    Frame::default().inner_margin(8.0).show(ui, |ui| {
                        ui.horizontal_centered(|ui| {
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
                    })
                });

            CentralPanel::default().show(ui, |ui| {
                ui.with_layout(Layout::top_down_justified(Align::LEFT), |ui| {
                    Frame::new().show(ui, |ui| match self.current_tab {
                        TabId::Library => {}
                        TabId::FileBrowser => {
                            ui.add(FileBrowser {
                                state: &mut self.file_browser_state,
                                machine_initialization_step: &mut self.machine_initialization_step,
                                program_manager: &self.program_manager,
                                toast_manager: &mut self.toast_manager,
                                external_file_dialog_supported: self.external_file_dialog_supported,
                            });
                        }
                        TabId::Settings => {
                            self.handle_settings(ui);
                        }
                        TabId::Log => {}
                        TabId::Controller => {}
                        TabId::Debug => {
                            if let Some(MachineContext {
                                simulation_controller,
                                ..
                            }) = &mut self.machine_context
                            {
                                ui.add(simulation_controller);
                            }
                        }
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
                                self.toast_manager
                                    .toast(ToastKind::Error, "Could not properly identify program");

                                return;
                            };

                            program_specification
                        };

                        self.build_machine_for_specification(specification);
                    }
                    Err(err) => {
                        self.toast_manager.toast(
                            ToastKind::Error,
                            format!("Failed to find matching specification: {}", err),
                        );
                    }
                }
            }
            MachineInitializationStep::BuildingMachineBuilder { job } if job.is_finished() => {
                if let Some(sealed) = job.join().unwrap() {
                    self.pending_machine = Some(sealed);
                } else {
                    self.toast_manager
                        .toast(ToastKind::Error, "Could not construct machine for program");
                }
            }
            unfinished => self.machine_initialization_step = Some(unfinished),
        }
    }
}

impl<P: FrontendPlatform> Drop for Frontend<P> {
    // Save on exit
    fn drop(&mut self) {
        if let Ok(environment) = ron::to_string(&self.environment).map_err(|err| {
            tracing::error!("Could not serialize environment: {}", err);
        }) {
            let Err(err) = std::fs::write(ENVIRONMENT_LOCATION.as_path(), environment) else {
                return;
            };

            tracing::error!("Could not save environment: {}", err);
        }
    }
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

#[inline]
fn to_egui_color(color: impl Into<Srgba<u8>>) -> Color32 {
    let color = color.into();

    Color32::from_rgba_unmultiplied(color.red, color.green, color.blue, color.alpha)
}
