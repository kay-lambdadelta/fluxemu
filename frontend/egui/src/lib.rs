//! Generic multiplatform frontend implementation for fluxemu

mod file_browser;
mod input;
mod menu_panel;
pub mod rendering;
mod settings;
mod simulation_controller;
mod toast;
mod utils;

use std::{borrow::Cow, collections::HashMap, ops::Deref, sync::Arc, thread::JoinHandle};

use egui::{
    Align, CentralPanel, FontDefinitions, Frame, FullOutput, Id, Layout, RawInput, Rect, Sense,
    pos2, vec2,
};
use egui_toast::ToastKind;
use fluxemu_environment::{ENVIRONMENT_LOCATION, Environment};
use fluxemu_frontend::{
    Platform,
    audio::{AudioRuntime, mixer::AudioMixer},
    machine::FactoryManager,
    simulation_controller::Controller,
};
use fluxemu_graphics::api::{Framebuffer, GraphicsApi};
use fluxemu_input::{InputId, InputState, physical::PhysicalInputDeviceId};
use fluxemu_program::{Manifest, ProgramManager, RomId};
use fluxemu_runtime::{
    ResourcePath,
    machine::{Machine, builder::SealedMachineBuilder},
};
use indexmap::{IndexMap, IndexSet};
use nalgebra::Vector2;
use palette::{
    WithAlpha,
    named::{BLACK, WHITE},
};
use ron::ser::PrettyConfig;

use crate::{
    file_browser::{FileBrowser, FileBrowserState},
    input::translator::EguiInputTranslator,
    menu_panel::{MenuPanel, TabId},
    rendering::EguiCapableGraphicsRuntime,
    toast::ToastManager,
    utils::{allocate_fill_aspect, setup_egui_context, to_egui_color},
};

rust_i18n::i18n!("locales", fallback = "en");

#[derive(Debug, Clone)]
struct PhysicalInputDeviceState {
    /// Should the runtime translate this input device into something egui can understand
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
        job: JoinHandle<Result<Vec<Manifest>, fluxemu_program::Error>>,
    },
    /// Step 3: Create and seal a machine builder given the specification
    BuildingMachineBuilder {
        job: JoinHandle<Option<SealedMachineBuilder<P>>>,
    },
}

pub struct Frontend<P: Platform> {
    machine_context: Option<MachineContext>,
    pending_machine: Option<SealedMachineBuilder<P>>,
    machine_factory_manager: Arc<FactoryManager<P>>,
    program_manager: Arc<ProgramManager>,
    physical_input_devices: HashMap<PhysicalInputDeviceId, PhysicalInputDeviceState>,
    egui_context: egui::Context,
    file_browser_state: FileBrowserState,
    menu_panel: MenuPanel,
    machine_initialization_step: Option<MachineInitializationStep<P>>,
    toast_manager: ToastManager,
    font_definitions: FontDefinitions,
    egui_input_translator: EguiInputTranslator,
    #[allow(unused)]
    audio_runtime: P::AudioRuntime,
    audio_mixer: Arc<AudioMixer>,
    /// ID of area where framebuffers will be drawn
    machine_framebuffer_display_area: Id,
    environment: Environment,
}

impl<P: Platform> Frontend<P> {
    pub fn new(
        environment: Environment,
        machine_factories: FactoryManager<P>,
        program_manager: Arc<ProgramManager>,
        mut audio_runtime: P::AudioRuntime,
        initial_program: Option<Vec<RomId>>,
        font_definitions: FontDefinitions,
    ) -> Self {
        let initial_program_initialization_step = initial_program.map(|roms| {
            let program_manager = program_manager.clone();

            MachineInitializationStep::FindingMatchingSpecification {
                roms: roms.clone(),
                job: std::thread::spawn(move || program_manager.identify_program(roms)),
            }
        });

        let sample_rate = audio_runtime.sample_rate();
        let audio_mixer = Arc::new(AudioMixer::new(sample_rate, environment.audio.volume));
        audio_runtime.set_audio_mixer(audio_mixer.clone());

        Self {
            machine_context: None,
            pending_machine: None,
            audio_runtime,
            machine_factory_manager: Arc::new(machine_factories),
            program_manager,
            menu_panel: MenuPanel::default(),
            physical_input_devices: HashMap::default(),
            egui_context: setup_egui_context(font_definitions.clone()),
            audio_mixer,
            file_browser_state: FileBrowserState::new(
                environment.file_browser_home_directory.clone(),
            ),
            toast_manager: ToastManager::default(),
            machine_initialization_step: initial_program_initialization_step,
            environment,
            font_definitions,
            egui_input_translator: EguiInputTranslator::default(),
            machine_framebuffer_display_area: Id::new(format!(
                "{}/machine_framebuffer_display_area",
                env!("CARGO_CRATE_NAME")
            )),
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

    fn bring_down_current_machine(&mut self) {
        self.machine_context = None;

        // Let go of any registered input devices
        for physical_gamepad_state in self.physical_input_devices.values_mut() {
            physical_gamepad_state.controlling_input_devices.clear();
        }
    }

    fn build_machine_for_specification(&mut self, specification: Manifest) {
        let program_manager = self.program_manager.clone();
        let machine_factories = self.machine_factory_manager.clone();

        let machine_builder = Machine::build(Some(specification), program_manager);

        let handle = std::thread::spawn(move || {
            machine_factories
                .construct_machine(ron::Value::from(()), machine_builder)
                .map(|result| result.unwrap())
        });

        self.machine_initialization_step =
            Some(MachineInitializationStep::BuildingMachineBuilder { job: handle });
    }

    pub fn maybe_reset_graphics_to_meet_machine_requirements(
        &mut self,
        callback: impl FnOnce(
            &egui::Context,
            &SealedMachineBuilder<P>,
        ) -> <P::GraphicsApi as GraphicsApi>::InitializationData,
    ) {
        if let Some(sealed_machine_builder) = self.pending_machine.take() {
            self.egui_context = setup_egui_context(self.font_definitions.clone());

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

            let controller = Controller::new(machine.clone(), self.audio_mixer.clone());

            // Make sure the simulation is currently running
            controller.set_paused(false);

            self.machine_context = Some(MachineContext {
                controller,
                machine,
                controller_ui_state: simulation_controller::State::default(),
            });

            self.set_machine_focus(true);
            self.menu_panel.is_expanded = false;
            self.menu_panel.current_tab = TabId::Machine;
        }
    }

    pub fn run(
        &mut self,
        mut external_input: RawInput,
        graphics_runtime: &mut P::GraphicsRuntime,
    ) -> FullOutput
    where
        P::GraphicsRuntime: EguiCapableGraphicsRuntime,
    {
        external_input
            .events
            .extend(self.egui_input_translator.drain_events());

        self.egui_context.clone().run_ui(external_input, |ui| {
            if let Some(machine_initialization_step) = self.machine_initialization_step.take() {
                self.service_machine_initialization_step(machine_initialization_step);
            }

            self.toast_manager.show(ui);

            ui.add(&mut self.menu_panel);

            // Remove the margin on the machine tab
            let panel_frame = if let TabId::Machine = self.menu_panel.current_tab {
                Frame::central_panel(ui.style()).inner_margin(0.0)
            } else {
                Frame::central_panel(ui.style())
            };

            CentralPanel::default().frame(panel_frame).show(ui, |ui| {
                ui.with_layout(Layout::top_down_justified(Align::LEFT), |ui| {
                    Frame::new().show(ui, |ui| match self.menu_panel.current_tab {
                        TabId::Machine => {
                            self.draw_machine_framebuffers(ui, graphics_runtime);
                        }
                        TabId::Library => {}
                        TabId::FileBrowser => {
                            ui.add(FileBrowser {
                                state: &mut self.file_browser_state,
                                machine_initialization_step: &mut self.machine_initialization_step,
                                program_manager: &self.program_manager,
                                toast_manager: &mut self.toast_manager,
                            });
                        }
                        TabId::Settings => {
                            self.handle_settings(ui);
                        }
                        TabId::Log => {}
                        TabId::Controller => {}
                        TabId::Debug => {
                            if let Some(MachineContext {
                                controller,
                                controller_ui_state: ui_state,
                                ..
                            }) = &mut self.machine_context
                            {
                                ui_state.update(controller);
                                ui.add(ui_state);
                            }
                        }
                        TabId::About => {}
                    });
                });
            });
        })
    }

    fn draw_machine_framebuffers(
        &mut self,
        ui: &mut egui::Ui,
        graphics_runtime: &mut P::GraphicsRuntime,
    ) where
        P::GraphicsRuntime: EguiCapableGraphicsRuntime,
    {
        if let Some(MachineContext {
            machine,
            controller,
            ..
        }) = &self.machine_context
        {
            let runtime = machine.enter_runtime();
            let framebuffer = runtime.list_framebuffers().next().unwrap();

            let callback = runtime
                .read_framebuffer::<P::GraphicsApi, _>(
                    framebuffer,
                    &runtime.safe_advance_timestamp(),
                    |framebuffer| {
                        let size =
                            Vector2::new(framebuffer.width() as f32, framebuffer.height() as f32);

                        let (rect, response) = allocate_fill_aspect(
                            ui,
                            size,
                            self.machine_framebuffer_display_area,
                            Sense::click(),
                        );

                        if response.clicked() {
                            self.menu_panel.is_expanded = false;
                            controller.set_paused(false);
                        }

                        graphics_runtime.produce_callback_for_framebuffer(rect, framebuffer)
                    },
                )
                .unwrap();

            let rect = callback.rect;
            let painter = ui.painter_at(rect);

            painter.add(callback);

            if controller.get_paused() {
                // Grey out framebuffer
                painter.rect_filled(rect, 0.0, to_egui_color(BLACK.with_alpha(140)));

                // Draw rudimentary pause symbol
                let center = rect.center();
                let bar_height = rect.width().min(rect.height()) * 0.2;
                let bar_width = bar_height * 0.3;
                let gap = bar_height * 0.25;

                for side in [-1.0, 1.0] {
                    let bar_center =
                        pos2(center.x + side * (gap / 2.0 + bar_width / 2.0), center.y);
                    let bar = Rect::from_center_size(bar_center, vec2(bar_width, bar_height));

                    painter.rect_filled(bar, bar_width * 0.2, to_egui_color(WHITE.with_alpha(230)));
                }
            } else {
                // Repaint as soon as possible, in reality limited to vsync
                self.egui_context.request_repaint();
            }
        }
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
                                    program_manager.identify_program(roms)
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

    fn save_environment(&mut self) {
        if let Ok(environment) = ron::Options::default()
            .to_string_pretty(&self.environment, PrettyConfig::new())
            .map_err(|err| {
                tracing::error!("Could not serialize environment: {}", err);
            })
        {
            let Err(err) = std::fs::write(ENVIRONMENT_LOCATION.deref(), environment) else {
                return;
            };

            tracing::error!("Could not save environment: {}", err);
        }
    }

    /// Check if any framebuffer currently has UI focus
    pub fn machine_has_focus(&self) -> bool {
        self.egui_context
            .memory(|memory| memory.has_focus(self.machine_framebuffer_display_area))
    }

    /// Set the focus state of the machine
    pub fn set_machine_focus(&mut self, focus: bool) {
        self.egui_context.memory_mut(|memory| {
            if focus {
                memory.request_focus(self.machine_framebuffer_display_area);
            } else {
                memory.surrender_focus(self.machine_framebuffer_display_area);
            }
        })
    }
}

impl<P: Platform> Drop for Frontend<P> {
    // Save on exit
    fn drop(&mut self) {
        self.save_environment();
    }
}

struct MachineContext {
    machine: Arc<Machine>,
    controller: Controller,
    controller_ui_state: simulation_controller::State,
}
