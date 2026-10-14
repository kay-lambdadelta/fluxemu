use std::{collections::HashMap, ops::Deref, path::PathBuf, sync::Arc, thread::JoinHandle};

use fluxemu_environment::{ENVIRONMENT_LOCATION, Environment};
use fluxemu_graphics::api::GraphicsApi;
use fluxemu_input::{InputState, physical::PhysicalInputDeviceId};
use fluxemu_program::{Manifest, ProgramManager, RomId};
use fluxemu_runtime::machine::{Machine, builder::SealedMachineBuilder};
use ron::ser::PrettyConfig;
use rust_i18n::t;

pub use self::input::InputRouting;
use self::input::PhysicalInputDeviceState;
use crate::{
    Platform,
    audio::{AudioRuntime, mixer::AudioMixer},
    machine::FactoryManager,
    simulation_controller::Controller,
};

mod input;

#[derive(Debug, Clone)]
pub enum FrontendEvent {
    MenuToggleRequested,
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

struct MachineContext {
    machine: Arc<Machine>,
    controller: Controller,
}

pub struct FrontendCore<P: Platform> {
    machine_context: Option<MachineContext>,
    pending_machine: Option<SealedMachineBuilder<P>>,
    machine_initialization_step: Option<MachineInitializationStep<P>>,
    machine_factory_manager: Arc<FactoryManager<P>>,
    program_manager: Arc<ProgramManager>,
    physical_input_devices: HashMap<PhysicalInputDeviceId, PhysicalInputDeviceState>,
    #[allow(unused)]
    audio_runtime: P::AudioRuntime,
    audio_mixer: Arc<AudioMixer>,
    events: Vec<FrontendEvent>,
    environment: Environment,
}

impl<P: Platform> FrontendCore<P> {
    pub fn new(
        environment: Environment,
        machine_factories: FactoryManager<P>,
        program_manager: Arc<ProgramManager>,
        mut audio_runtime: P::AudioRuntime,
        initial_program: Option<Vec<RomId>>,
    ) -> Self {
        let sample_rate = audio_runtime.sample_rate();
        let audio_mixer = Arc::new(AudioMixer::new(sample_rate, environment.audio.volume));
        audio_runtime.set_audio_mixer(audio_mixer.clone());

        let mut core = Self {
            machine_context: None,
            pending_machine: None,
            machine_initialization_step: None,
            machine_factory_manager: Arc::new(machine_factories),
            program_manager,
            physical_input_devices: HashMap::default(),
            audio_runtime,
            audio_mixer,
            events: Vec::default(),
            environment,
        };

        if let Some(roms) = initial_program {
            core.identify_program(roms);
        }

        core
    }

    pub fn environment(&self) -> &Environment {
        &self.environment
    }

    pub fn environment_mut(&mut self) -> &mut Environment {
        &mut self.environment
    }

    pub fn program_manager(&self) -> &Arc<ProgramManager> {
        &self.program_manager
    }

    pub fn machine(&self) -> Option<&Arc<Machine>> {
        self.machine_context
            .as_ref()
            .map(|context| &context.machine)
    }

    pub fn controller(&self) -> Option<&Controller> {
        self.machine_context
            .as_ref()
            .map(|context| &context.controller)
    }

    pub fn set_volume(&mut self, volume: f32) {
        self.environment.audio.volume = volume;
        self.audio_mixer.set_volume(volume);
    }

    pub fn drain_events(&mut self) -> impl Iterator<Item = FrontendEvent> + use<P> {
        std::mem::take(&mut self.events).into_iter()
    }

    /// Begin the process of turning these files into a running machine
    pub fn load_roms(&mut self, paths: Vec<PathBuf>) {
        let program_manager = self.program_manager.clone();

        let job = std::thread::Builder::new()
            .name(t!("machine_initialization.thread_name_rom_id_calculator").to_string())
            .spawn(move || {
                paths
                    .into_iter()
                    .map(|path| program_manager.register_external_rom(path))
                    .collect()
            })
            .expect("Could not launch thread to process ROM for its ID!");

        self.machine_initialization_step =
            Some(MachineInitializationStep::CalculatingRomIds { job });
    }

    /// Advance any in progress machine initialization
    pub fn poll(&mut self) {
        let Some(step) = self.machine_initialization_step.take() else {
            return;
        };

        match step {
            MachineInitializationStep::CalculatingRomIds { job } if job.is_finished() => {
                match job.join().unwrap() {
                    Ok(roms) => self.identify_program(roms),
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
                    Err(err) => {
                        tracing::error!("Failed to find matching specification: {}", err);
                    }
                }
            }
            MachineInitializationStep::BuildingMachineBuilder { job } if job.is_finished() => {
                if let Some(sealed) = job.join().unwrap() {
                    self.pending_machine = Some(sealed);
                } else {
                    tracing::error!("Could not construct machine for program");
                }
            }
            unfinished => self.machine_initialization_step = Some(unfinished),
        }
    }

    pub fn has_pending_machine(&self) -> bool {
        self.pending_machine.is_some()
    }

    /// Replace the current machine with the pending one, returning if the machine started
    pub fn start_pending_machine(
        &mut self,
        initialize_graphics: impl FnOnce(
            &SealedMachineBuilder<P>,
        ) -> <P::GraphicsApi as GraphicsApi>::InitializationData,
    ) -> bool {
        let Some(sealed_machine_builder) = self.pending_machine.take() else {
            return false;
        };

        // NOTE: This will block the ui
        self.bring_down_current_machine();

        let graphics_initialization_data = initialize_graphics(&sealed_machine_builder);

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
        });

        true
    }

    pub fn pause_machine(&mut self) {
        if let Some(controller) = self.controller() {
            controller.set_paused(true);
        }
    }

    /// Unpause the machine, releasing all of its inputs so nothing is held from before the pause
    pub fn resume_machine(&mut self) {
        let Some(MachineContext {
            machine,
            controller,
        }) = &self.machine_context
        else {
            return;
        };

        // Enter runtime
        let runtime_guard = machine.enter_runtime();

        // Unset ALL inputs
        for (logical_input_device_path, logical_input_device) in runtime_guard.input_devices() {
            let unset_inputs = logical_input_device
                .metadata()
                .present_inputs
                .iter()
                .copied()
                .map(|input_id| (input_id, InputState::RELEASED));

            runtime_guard.insert_inputs(logical_input_device_path, unset_inputs);
        }

        controller.set_paused(false);
    }

    pub fn save_environment(&self) {
        let environment = match ron::Options::default()
            .to_string_pretty(&self.environment, PrettyConfig::new())
        {
            Ok(environment) => environment,
            Err(err) => {
                tracing::error!("Could not serialize environment: {}", err);
                return;
            }
        };

        if let Err(err) = std::fs::write(ENVIRONMENT_LOCATION.deref(), environment) {
            tracing::error!("Could not save environment: {}", err);
        }
    }

    fn identify_program(&mut self, roms: Vec<RomId>) {
        let program_manager = self.program_manager.clone();

        self.machine_initialization_step =
            Some(MachineInitializationStep::FindingMatchingSpecification {
                roms: roms.clone(),
                job: std::thread::spawn(move || program_manager.identify_program(roms)),
            });
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

    fn bring_down_current_machine(&mut self) {
        self.machine_context = None;

        // Let go of any registered input devices
        for physical_gamepad_state in self.physical_input_devices.values_mut() {
            physical_gamepad_state.controlling_input_devices.clear();
        }
    }
}

impl<P: Platform> Drop for FrontendCore<P> {
    // Save on exit
    fn drop(&mut self) {
        self.save_environment();
    }
}
