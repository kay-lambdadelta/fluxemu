use std::sync::Arc;

use fluxemu_database::ProgramDatabase;
use fluxemu_environment::Environment;
use fluxemu_frontend::{FrontendCore, notification::NotificationReceiver};
use fluxemu_frontend_ratatui::{TerminalGraphicsRuntime, external_input_channel};
use fluxemu_program::ProgramManager;
use tokio::runtime::Handle;

use crate::{build_machine::get_software_factories, platform::Platform, runtime::AudioRuntime};

pub fn run(
    runtime: Handle,
    environment: Environment,
    program_manager: Arc<ProgramManager>,
    program_database: Arc<ProgramDatabase>,
    notifications: NotificationReceiver,
) -> Result<(), Box<dyn std::error::Error>> {
    let (_external_input_sender, external_input_receiver) = external_input_channel();

    let core = FrontendCore::<Platform<TerminalGraphicsRuntime>>::new(
        runtime,
        environment,
        get_software_factories(),
        program_manager,
        program_database,
        AudioRuntime,
        None,
    );

    fluxemu_frontend_ratatui::run(core, notifications, external_input_receiver)
}
