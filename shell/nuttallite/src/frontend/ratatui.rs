use std::sync::Arc;

use fluxemu_environment::Environment;
use fluxemu_frontend::notification::NotificationReceiver;
use fluxemu_frontend_ratatui::{TerminalGraphicsRuntime, external_input_channel};
use fluxemu_program::ProgramManager;

use crate::{build_machine::get_software_factories, platform::Platform, runtime::AudioRuntime};

pub fn run(
    environment: Environment,
    program_manager: Arc<ProgramManager>,
    notifications: NotificationReceiver,
) -> Result<(), Box<dyn std::error::Error>> {
    let (_external_input_sender, external_input_receiver) = external_input_channel();

    fluxemu_frontend_ratatui::run::<Platform<TerminalGraphicsRuntime>>(
        environment,
        program_manager,
        get_software_factories(),
        AudioRuntime,
        None,
        notifications,
        external_input_receiver,
    )
}
