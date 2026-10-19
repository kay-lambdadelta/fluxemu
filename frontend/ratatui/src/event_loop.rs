use std::{
    io::stdout,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

use crossterm::{
    event::{
        self, DisableFocusChange, EnableFocusChange, KeyboardEnhancementFlags,
        PopKeyboardEnhancementFlags, PushKeyboardEnhancementFlags,
    },
    execute,
    terminal::supports_keyboard_enhancement,
};
use fluxemu_environment::Environment;
use fluxemu_frontend::{
    FrontendCore, Platform, machine::FactoryManager, notification::NotificationReceiver,
};
use fluxemu_graphics::api::software::Software;
use fluxemu_program::{ProgramManager, RomId};
use ratatui::DefaultTerminal;
use ratatui_image::picker::Picker;

use crate::{
    external_input::{ExternalInputReceiver, ExternalInputSender, Message},
    frontend::Frontend,
};

const TERMINAL_POLL_INTERVAL: Duration = Duration::from_millis(100);

pub fn run<P: Platform<GraphicsApi = Software>>(
    environment: Environment,
    program_manager: Arc<ProgramManager>,
    machine_factories: FactoryManager<P>,
    audio_runtime: P::AudioRuntime,
    initial_program: Option<Vec<RomId>>,
    notifications: NotificationReceiver,
    external_input: ExternalInputReceiver,
) -> Result<(), Box<dyn std::error::Error>> {
    let core = FrontendCore::new(
        environment,
        machine_factories,
        program_manager,
        audio_runtime,
        initial_program,
    );

    ratatui::run(|terminal| {
        let picker = Picker::from_query_stdio().unwrap_or_else(|err| {
            tracing::warn!(
                "Could not query terminal graphics support, falling back to halfblocks: {}",
                err
            );

            Picker::halfblocks()
        });

        tracing::info!(
            "For machine display, {:?} protocol was chosen",
            picker.protocol_type()
        );

        let reports_key_releases = supports_keyboard_enhancement().unwrap_or(false);

        if reports_key_releases {
            execute!(
                stdout(),
                PushKeyboardEnhancementFlags(
                    KeyboardEnhancementFlags::DISAMBIGUATE_ESCAPE_CODES
                        | KeyboardEnhancementFlags::REPORT_EVENT_TYPES
                        | KeyboardEnhancementFlags::REPORT_ALL_KEYS_AS_ESCAPE_CODES
                )
            )?;
        } else {
            tracing::warn!(
                "Terminal does not report key releases, they will be approximated, this will be deeply unpleasant"
            );
        }

        execute!(stdout(), EnableFocusChange)?;

        let mut frontend = Frontend::new(core, notifications, picker, reports_key_releases)?;

        let should_stop_reading = Arc::new(AtomicBool::new(false));
        let terminal_reader = std::thread::Builder::new()
            .name("Terminal Input Thread".to_string())
            .spawn({
                let sender = external_input.sender();
                let should_stop_reading = should_stop_reading.clone();

                move || read_terminal_events(sender, &should_stop_reading)
            })?;

        let result = event_loop(terminal, &mut frontend, &external_input);

        should_stop_reading.store(true, Ordering::Relaxed);
        let _ = terminal_reader.join();

        execute!(stdout(), DisableFocusChange)?;

        if reports_key_releases {
            execute!(stdout(), PopKeyboardEnhancementFlags)?;
        }

        result
    })
}

fn event_loop<P: Platform<GraphicsApi = Software>>(
    terminal: &mut DefaultTerminal,
    frontend: &mut Frontend<P>,
    external_input: &ExternalInputReceiver,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut next_frame = Instant::now();

    loop {
        frontend.tick();

        if frontend.exit_requested() {
            return Ok(());
        }

        if frontend.take_redraw_request() || Instant::now() >= next_frame {
            if frontend.take_clear_request() {
                terminal.clear()?;
            }

            terminal.draw(|frame| frontend.draw(frame))?;
            next_frame = Instant::now() + frontend.frame_interval();
        }

        let deadline = frontend
            .next_deadline()
            .map_or(next_frame, |deadline| deadline.min(next_frame));

        let mut message =
            external_input.recv_timeout(deadline.saturating_duration_since(Instant::now()));

        while let Some(current_message) = message {
            match current_message {
                Message::TerminalError(err) => return Err(err.into()),
                current_message => frontend.handle_message(current_message),
            }

            message = external_input.try_recv();
        }
    }
}

fn read_terminal_events(sender: ExternalInputSender, should_stop: &AtomicBool) {
    while !should_stop.load(Ordering::Relaxed) {
        let event = match event::poll(TERMINAL_POLL_INTERVAL) {
            Ok(true) => event::read(),
            Ok(false) => continue,
            Err(err) => Err(err),
        };

        match event {
            Ok(event) => sender.send(Message::Terminal(event)),
            Err(err) => {
                sender.send(Message::TerminalError(err));
                return;
            }
        }
    }
}
