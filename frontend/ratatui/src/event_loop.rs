use std::sync::Arc;

use crossterm::event::{self, Event, KeyCode};
use fluxemu_environment::Environment;
use fluxemu_frontend::{Platform, machine::FactoryManager};
use fluxemu_program::{ProgramManager, RomId};
use ratatui::widgets::FrameExt;
use ratatui_explorer::{FileExplorerBuilder, Theme};
use ratatui_image::picker::Picker;

pub fn run<P: Platform>(
    _environment: Environment,
    _program_manager: Arc<ProgramManager>,
    _machine_factories: FactoryManager<P>,
    _initial_program: Option<Vec<RomId>>,
) -> Result<(), Box<dyn std::error::Error>> {
    ratatui::run(|terminal| {
        let _graphics_picker = Picker::from_query_stdio()?;

        let theme = Theme::default().add_default_title();
        let mut file_explorer = FileExplorerBuilder::default().theme(theme).build()?;

        loop {
            terminal.draw(|frame| {
                frame.render_widget_ref(file_explorer.widget(), frame.area());
            })?;

            let event = event::read()?;
            if let Event::Key(key) = event
                && key.code == KeyCode::Char('q')
            {
                break;
            }

            file_explorer.handle(&event)?;
        }

        Ok(())
    })
}
