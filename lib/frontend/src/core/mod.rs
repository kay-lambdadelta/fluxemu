use std::{
    collections::HashMap,
    num::Wrapping,
    ops::Deref,
    path::{Path, PathBuf},
    sync::Arc,
};

use fluxemu_database::ProgramDatabase;
use fluxemu_environment::{ENVIRONMENT_LOCATION, Environment};
use fluxemu_graphics::{
    api::GraphicsApi,
    texture::{CopyMode, OwnedTexture},
};
use fluxemu_input::{InputState, physical::PhysicalInputDeviceInstanceId};
use fluxemu_program::{ProgramManager, RomId, RomStoreSpecification};
use fluxemu_runtime::machine::{Machine, builder::SealedMachineBuilder};
use indexmap::IndexMap;
use palette::{Srgba, named::BLACK};
use ron::ser::PrettyConfig;
use tokio::{runtime::Handle, task::JoinHandle};

pub use self::input::InputRouting;
use self::input::PhysicalInputDeviceState;
use crate::{
    Platform,
    audio::{AudioRuntime, mixer::AudioMixer},
    graphics::GraphicsRuntime,
    machine::FactoryManager,
    metadata::{
        SlotInfo, SlotKind, SlotMetadata, delete_slot, list_slots, next_free_slot, read_metadata,
        write_metadata,
    },
    save::{
        SaveFlusher, active_save_slot, open_save, program_directory, save_path,
        set_active_save_slot,
    },
    simulation_controller::Controller,
    snapshot,
};

mod input;

#[derive(Debug, Clone)]
pub enum FrontendEvent {
    MenuToggleRequested,
    DebugOverlayToggleRequested,
}

struct MachineContext {
    machine: Arc<Machine>,
    controller: Controller,
    _save_flusher: Option<SaveFlusher>,
}

pub struct FrontendCore<P: Platform> {
    runtime: Handle,
    machine_context: Option<MachineContext>,
    pending_machine: Option<SealedMachineBuilder<P>>,
    machine_initialization: Option<JoinHandle<Option<SealedMachineBuilder<P>>>>,
    machine_factory_manager: Arc<FactoryManager<P>>,
    program_manager: Arc<ProgramManager>,
    program_database: Arc<ProgramDatabase>,
    physical_input_devices: HashMap<PhysicalInputDeviceInstanceId, PhysicalInputDeviceState>,
    #[allow(unused)]
    audio_runtime: P::AudioRuntime,
    audio_mixer: Arc<AudioMixer>,
    events: Vec<FrontendEvent>,
    pending_snapshot_stores: Vec<u8>,
    environment: Environment,
}

impl<P: Platform> FrontendCore<P> {
    pub fn new(
        runtime: Handle,
        environment: Environment,
        machine_factories: FactoryManager<P>,
        program_manager: Arc<ProgramManager>,
        program_database: Arc<ProgramDatabase>,
        mut audio_runtime: P::AudioRuntime,
        initial_program: Option<Vec<RomId>>,
    ) -> Self {
        let sample_rate = audio_runtime.sample_rate();
        let audio_mixer = Arc::new(AudioMixer::new(sample_rate, environment.audio.volume));
        audio_runtime.set_audio_mixer(audio_mixer.clone());

        let mut core = Self {
            runtime,
            machine_context: None,
            pending_machine: None,
            machine_initialization: None,
            machine_factory_manager: Arc::new(machine_factories),
            program_manager,
            program_database,
            physical_input_devices: HashMap::default(),
            audio_runtime,
            audio_mixer,
            events: Vec::default(),
            pending_snapshot_stores: Vec::default(),
            environment,
        };

        if let Some(roms) = initial_program {
            core.start_machine_initialization(async move { Some(roms) });
        }

        core
    }

    pub fn runtime(&self) -> &Handle {
        &self.runtime
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

    pub fn program_database(&self) -> &Arc<ProgramDatabase> {
        &self.program_database
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

    pub fn set_rom_stores(
        &mut self,
        rom_stores: IndexMap<String, RomStoreSpecification>,
    ) -> Result<(), fluxemu_program::Error> {
        self.program_manager.set_rom_stores(rom_stores.clone())?;
        self.environment.rom_stores = rom_stores;

        Ok(())
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

        self.start_machine_initialization(async move {
            let mut roms = Vec::with_capacity(paths.len());

            for path in paths {
                match program_manager.register_external_rom(path).await {
                    Ok(rom) => roms.push(rom),
                    Err(err) => {
                        tracing::error!("Failed to calculate ROM ids: {}", err);

                        return None;
                    }
                }
            }

            Some(roms)
        });
    }

    /// Advance any in progress machine initialization
    pub fn poll(&mut self, graphics_runtime: &P::GraphicsRuntime) {
        for slot in std::mem::take(&mut self.pending_snapshot_stores) {
            self.store_snapshot(slot, graphics_runtime);
        }

        let Some(job) = self.machine_initialization.take_if(|job| job.is_finished()) else {
            return;
        };

        if let Some(sealed) = self.runtime.block_on(job).unwrap() {
            self.pending_machine = Some(sealed);
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
        if let Some(logical_input_device_path) = runtime_guard.input_devices().keys().min() {
            for physical_input_device_state in self.physical_input_devices.values_mut() {
                physical_input_device_state
                    .controlling_input_devices
                    .insert(logical_input_device_path.clone());
            }
        }

        // Exit runtime
        drop(runtime_guard);

        let save_flusher = machine.program_specification().and_then(|specification| {
            let path = save_path(&self.environment.save_directory, &specification.id);

            let baseline = open_save(&machine, &path, self.environment.check_archive_hashes)?;

            Some(SaveFlusher::new(
                machine.clone(),
                path,
                self.environment.save_flush_interval,
                baseline,
            ))
        });

        let controller = Controller::new(machine.clone(), self.audio_mixer.clone());

        // Make sure the simulation is currently running
        controller.set_paused(false);

        self.machine_context = Some(MachineContext {
            controller,
            machine,
            _save_flusher: save_flusher,
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
            ..
        }) = &self.machine_context
        else {
            return;
        };

        release_all_inputs(machine);

        controller.set_paused(false);
    }

    fn slot_directory(&self, kind: SlotKind) -> Option<PathBuf> {
        let specification = self.machine()?.program_specification()?;

        let root = match kind {
            SlotKind::Save => &self.environment.save_directory,
            SlotKind::Snapshot => &self.environment.snapshot_directory,
        };

        Some(program_directory(root, &specification.id))
    }

    pub fn slot_metadata(&self, kind: SlotKind, slot: u8) -> SlotMetadata {
        self.slot_directory(kind)
            .map(|directory| read_metadata(&directory, slot))
            .unwrap_or_default()
    }

    pub fn set_slot_metadata(&self, kind: SlotKind, slot: u8, metadata: &SlotMetadata) {
        let Some(directory) = self.slot_directory(kind) else {
            return;
        };

        if let Err(error) = write_metadata(&directory, slot, metadata) {
            tracing::error!("Could not write metadata for slot {}: {}", slot, error);
        }
    }

    pub fn delete_slot(&self, kind: SlotKind, slot: u8) {
        let Some(directory) = self.slot_directory(kind) else {
            return;
        };

        if kind == SlotKind::Save && slot == active_save_slot(&directory) {
            tracing::warn!("Cannot delete the save slot currently in use");

            return;
        }

        if let Err(error) = delete_slot(&directory, slot) {
            tracing::error!("Could not delete slot {}: {}", slot, error);
        }
    }

    pub fn list_slots(&self, kind: SlotKind) -> Vec<SlotInfo> {
        self.slot_directory(kind)
            .map(|directory| list_slots(&directory, kind))
            .unwrap_or_default()
    }

    pub fn next_free_slot(&self, kind: SlotKind) -> Option<u8> {
        next_free_slot(&self.slot_directory(kind)?, kind)
    }

    pub fn slot_screenshot(&self, slot: u8) -> Option<OwnedTexture<Srgba<u8>>> {
        snapshot::read_screenshot(&self.slot_directory(SlotKind::Snapshot)?, slot)
    }

    pub fn active_snapshot_slot(&self) -> u8 {
        self.environment.active_snapshot_slot.0
    }

    pub fn set_active_snapshot_slot(&mut self, slot: u8) {
        self.environment.active_snapshot_slot = Wrapping(slot);
    }

    pub fn store_snapshot(&self, slot: u8, graphics_runtime: &P::GraphicsRuntime) {
        let (Some(context), Some(directory)) = (
            &self.machine_context,
            self.slot_directory(SlotKind::Snapshot),
        ) else {
            return;
        };

        let capture_screenshots = self.environment.snapshot_screenshots;

        snapshot::store_snapshot(&context.machine, directory, slot, &self.runtime, |guard| {
            if !capture_screenshots {
                return None;
            }

            guard
                .read_primary_framebuffer::<P::GraphicsApi, _>(|framebuffer, display_geometry| {
                    let display_rectangle = display_geometry.resolve_display_rectangle(framebuffer);
                    let screenshot = graphics_runtime.screenshot(framebuffer)?;
                    let screenshot =
                        screenshot.view(display_rectangle.x_range(), display_rectangle.y_range());

                    let mut cropped = OwnedTexture::from_value(
                        screenshot.width(),
                        screenshot.height(),
                        BLACK.into(),
                    );
                    cropped.copy_from(screenshot, CopyMode::Nearest);

                    Some(cropped)
                })
                .flatten()
        });
    }

    fn load_snapshot_file(&self, source: impl FnOnce(&Path) -> PathBuf) -> bool {
        let (Some(context), Some(directory)) = (
            &self.machine_context,
            self.slot_directory(SlotKind::Snapshot),
        ) else {
            return false;
        };

        snapshot::load_snapshot(
            &context.machine,
            &self.audio_mixer,
            &source(&directory),
            &directory,
            self.environment.check_archive_hashes,
            &self.runtime,
        )
        .is_some()
    }

    pub fn load_snapshot(&self, slot: u8) {
        if self.load_snapshot_file(|directory| snapshot::snapshot_path(directory, slot)) {
            tracing::info!("Loaded snapshot from slot {}", slot);
        }
    }

    pub fn can_undo_load_snapshot(&self) -> bool {
        self.slot_directory(SlotKind::Snapshot)
            .is_some_and(|directory| snapshot::undo_load_path(&directory).exists())
    }

    pub fn undo_load_snapshot(&self) {
        if self.load_snapshot_file(snapshot::undo_load_path) {
            tracing::info!("Undid snapshot load");
        }
    }

    pub fn can_undo_store_snapshot(&self) -> bool {
        self.slot_directory(SlotKind::Snapshot)
            .is_some_and(|directory| snapshot::can_undo_store(&directory))
    }

    pub fn undo_store_snapshot(&self) {
        let Some(directory) = self.slot_directory(SlotKind::Snapshot) else {
            return;
        };

        match snapshot::undo_store(&directory) {
            Ok(Some(slot)) => tracing::info!("Undid snapshot store in slot {}", slot),
            Ok(None) => tracing::warn!("No snapshot store to undo"),
            Err(error) => tracing::error!("Could not undo snapshot store: {}", error),
        }
    }

    pub fn active_save_slot(&self) -> u8 {
        self.slot_directory(SlotKind::Save)
            .map_or(0, |directory| active_save_slot(&directory))
    }

    pub fn use_save_slot(&mut self, slot: u8) {
        let Some(directory) = self.slot_directory(SlotKind::Save) else {
            return;
        };

        let Some(specification) = self
            .machine()
            .and_then(|machine| machine.program_specification())
        else {
            return;
        };

        let roms: Vec<_> = specification.info.filesystem().keys().copied().collect();

        if let Err(error) = set_active_save_slot(&directory, slot) {
            tracing::error!("Could not switch save slot: {}", error);

            return;
        }

        self.start_machine_initialization(async move { Some(roms) });
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

    fn start_machine_initialization(
        &mut self,
        roms: impl Future<Output = Option<Vec<RomId>>> + Send + 'static,
    ) {
        let program_manager = self.program_manager.clone();
        let program_database = self.program_database.clone();
        let machine_factories = self.machine_factory_manager.clone();

        self.machine_initialization = Some(self.runtime.spawn(async move {
            initialize_machine(
                program_manager,
                program_database,
                machine_factories,
                roms.await?,
            )
            .await
        }));
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

pub(crate) fn release_all_inputs(machine: &Arc<Machine>) {
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
}

async fn initialize_machine<P: Platform>(
    program_manager: Arc<ProgramManager>,
    program_database: Arc<ProgramDatabase>,
    machine_factories: Arc<FactoryManager<P>>,
    roms: Vec<RomId>,
) -> Option<SealedMachineBuilder<P>> {
    let specifications = tokio::task::spawn_blocking({
        let roms = roms.clone();

        move || program_database.identify_program(roms)
    })
    .await
    .unwrap();

    let mut specifications = match specifications {
        Ok(specifications) => specifications,
        Err(err) => {
            tracing::error!("Failed to find matching specification: {}", err);

            return None;
        }
    };

    let specification = if !specifications.is_empty() {
        specifications.remove(0)
    } else {
        let Ok(Some(program_specification)) =
            program_manager.auto_generate_specification(roms[0]).await
        else {
            tracing::error!("Could not properly identify program");

            return None;
        };

        program_specification
    };

    let program_id = specification.id.clone();
    let machine_builder = Machine::build(Some(specification), program_manager);

    match machine_factories
        .construct_machine(ron::Value::from(()), machine_builder)
        .await
    {
        Some(Ok(sealed)) => Some(sealed),
        Some(Err(err)) => {
            tracing::error!("Could not construct machine for {}: {}", program_id, err);

            None
        }
        None => {
            tracing::error!("Could not construct machine for {}", program_id);

            None
        }
    }
}
