use std::{
    collections::{HashMap, VecDeque},
    time::{Duration, Instant},
};

use crossterm::event::{Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use fluxemu_frontend::{
    FrontendCore, FrontendEvent, InputRouting, Platform,
    notification::{Notification, NotificationReceiver},
};
use fluxemu_graphics::api::software::Software;
use fluxemu_input::{GamepadInputId, InputId, InputState, physical::PhysicalInputDeviceId};
use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    style::{Color, Style, Stylize},
    text::{Line, Span},
    widgets::{Block, FrameExt, List, ListItem, ListState, Paragraph, Row, Table, Tabs},
};
use ratatui_explorer::{FileExplorer, FileExplorerBuilder, Theme};
use ratatui_image::picker::Picker;
use rust_i18n::t;
use strum::{AsRefStr, EnumCount, EnumIter, IntoEnumIterator, VariantArray};
use tracing::Level;

use crate::{
    external_input::Message,
    input::{SyntheticReleaseTracker, crossterm2key, gamepad2key},
    machine_view::MachineView,
};

const RUNNING_FRAME_INTERVAL: Duration = Duration::from_millis(33);
const IDLE_FRAME_INTERVAL: Duration = Duration::from_millis(250);
const MAX_LOG_ENTRIES: usize = 1000;
const STATUS_NOTIFICATION_DURATION: Duration = Duration::from_secs(5);
const KEYBOARD: PhysicalInputDeviceId = PhysicalInputDeviceId::PLATFORM_RESERVED;

#[derive(Debug, Clone, Copy, PartialEq, Eq, AsRefStr, EnumCount, EnumIter, VariantArray)]
#[strum(serialize_all = "snake_case", prefix = "tab.")]
enum Tab {
    Machine,
    Files,
    Log,
    Debug,
}

impl Tab {
    fn index(self) -> usize {
        self as usize
    }

    fn title(self) -> String {
        t!(self.as_ref()).to_string()
    }

    fn offset(self, offset: isize) -> Self {
        let index = (self.index() as isize + offset).rem_euclid(Self::COUNT as isize);

        Self::VARIANTS[index as usize]
    }
}

struct LogEntry {
    notification: Notification,
    received: Instant,
}

pub struct Frontend<P: Platform<GraphicsApi = Software>> {
    core: FrontendCore<P>,
    notifications: NotificationReceiver,
    picker: Picker,
    file_explorer: FileExplorer,
    machine_view: MachineView,
    current_tab: Tab,
    machine_has_focus: bool,
    log: VecDeque<LogEntry>,
    log_state: ListState,
    follow_log: bool,
    reports_key_releases: bool,
    synthetic_releases: SyntheticReleaseTracker,
    gamepad_ui_inputs_held: HashMap<(PhysicalInputDeviceId, GamepadInputId), bool>,
    redraw_requested: bool,
    clear_requested: bool,
    exit_requested: bool,
}

impl<P: Platform<GraphicsApi = Software>> Frontend<P> {
    pub fn new(
        mut core: FrontendCore<P>,
        notifications: NotificationReceiver,
        picker: Picker,
        reports_key_releases: bool,
    ) -> std::io::Result<Self> {
        let theme = Theme::default()
            .with_block(Block::bordered())
            .add_default_title()
            .with_highlight_item_style(Style::new().reversed())
            .with_highlight_dir_style(Style::new().reversed().bold())
            .with_dir_style(Style::new().bold());

        let file_explorer = FileExplorerBuilder::default()
            .working_dir(core.environment().file_browser_home_directory.clone())
            .theme(theme)
            .build()?;

        core.register_gamepad(KEYBOARD, "Keyboard", false);

        Ok(Self {
            core,
            notifications,
            picker,
            file_explorer,
            machine_view: MachineView::default(),
            current_tab: Tab::Files,
            machine_has_focus: false,
            log: VecDeque::default(),
            log_state: ListState::default(),
            follow_log: true,
            reports_key_releases,
            synthetic_releases: SyntheticReleaseTracker::default(),
            gamepad_ui_inputs_held: HashMap::default(),
            redraw_requested: true,
            clear_requested: false,
            exit_requested: false,
        })
    }

    pub fn exit_requested(&self) -> bool {
        self.exit_requested
    }

    pub fn take_redraw_request(&mut self) -> bool {
        std::mem::take(&mut self.redraw_requested)
    }

    pub fn take_clear_request(&mut self) -> bool {
        std::mem::take(&mut self.clear_requested)
    }

    pub fn frame_interval(&self) -> Duration {
        if self.current_tab == Tab::Machine && self.machine_is_running() {
            RUNNING_FRAME_INTERVAL
        } else {
            IDLE_FRAME_INTERVAL
        }
    }

    pub fn next_deadline(&self) -> Option<Instant> {
        self.synthetic_releases.next_deadline()
    }

    pub fn tick(&mut self) {
        self.core.poll();

        if self.core.has_pending_machine() && self.core.start_pending_machine(|_| ()) {
            self.machine_view.clear();
            self.switch_tab(Tab::Machine);
            self.machine_has_focus = true;
        }

        for key in self.synthetic_releases.drain_expired() {
            self.core.insert_input(
                KEYBOARD,
                InputId::Keyboard(key),
                InputState::RELEASED,
                self.machine_has_focus,
            );
        }

        self.handle_core_events();
    }

    pub fn handle_message(&mut self, message: Message) {
        match message {
            Message::Terminal(event) => self.handle_event(event),
            Message::GamepadConnected { id, name } => {
                self.core.register_gamepad(id, name, true);
            }
            Message::GamepadDisconnected { id } => {
                self.core.unregister_gamepad(id);
                self.gamepad_ui_inputs_held
                    .retain(|(device_id, _), _| *device_id != id);
            }
            Message::Input {
                id,
                input_id,
                state,
            } => self.handle_external_input(id, input_id, state),
            Message::TerminalError(_) => {}
        }
    }

    fn handle_external_input(
        &mut self,
        id: PhysicalInputDeviceId,
        input_id: InputId,
        state: InputState,
    ) {
        let routing = self
            .core
            .insert_input(id, input_id, state, self.machine_has_focus);
        self.handle_core_events();

        let InputId::Gamepad(gamepad_input_id) = input_id else {
            return;
        };

        let is_held = state.as_digital(None);
        let was_held = self
            .gamepad_ui_inputs_held
            .insert((id, gamepad_input_id), is_held)
            .unwrap_or(false);

        if routing == InputRouting::Ui
            && is_held
            && !was_held
            && let Some(code) = gamepad2key(gamepad_input_id)
        {
            let key = KeyEvent::from(code);

            self.handle_ui_key(key, &Event::Key(key));
            self.redraw_requested = true;
        }
    }

    fn handle_event(&mut self, event: Event) {
        match event {
            Event::Key(key) => self.handle_key(key, &event),
            Event::Resize(..) => {
                self.machine_view.clear();
                self.redraw_requested = true;
            }
            Event::FocusLost if self.machine_has_focus => {
                self.core.pause_machine();
                self.machine_has_focus = false;
                self.redraw_requested = true;
            }
            _ => {}
        }
    }

    fn handle_key(&mut self, key: KeyEvent, event: &Event) {
        let is_press = matches!(key.kind, KeyEventKind::Press | KeyEventKind::Repeat);

        if is_press
            && key.code == KeyCode::Char('c')
            && key.modifiers.contains(KeyModifiers::CONTROL)
        {
            self.exit_requested = true;
            return;
        }

        let ui_has_focus = !self.machine_has_focus;

        if let Some(keyboard_input_id) = crossterm2key(key.code) {
            if !self.reports_key_releases {
                self.synthetic_releases.press(keyboard_input_id);
            } else if !is_press {
                self.synthetic_releases.release(keyboard_input_id);
            }

            let state = if is_press {
                InputState::PRESSED
            } else {
                InputState::RELEASED
            };

            self.core.insert_input(
                KEYBOARD,
                InputId::Keyboard(keyboard_input_id),
                state,
                self.machine_has_focus,
            );
            self.handle_core_events();
        }

        if ui_has_focus && is_press {
            self.handle_ui_key(key, event);
            self.redraw_requested = true;
        }
    }

    fn handle_ui_key(&mut self, key: KeyEvent, event: &Event) {
        match key.code {
            KeyCode::Tab => return self.switch_tab(self.current_tab.offset(1)),
            KeyCode::BackTab => return self.switch_tab(self.current_tab.offset(-1)),
            KeyCode::Char(character @ '1'..='9') => {
                let index = character as usize - '1' as usize;

                if let Some(tab) = Tab::VARIANTS.get(index) {
                    return self.switch_tab(*tab);
                }
            }
            KeyCode::Char('q') => {
                self.exit_requested = true;
                return;
            }
            KeyCode::Esc => return self.resume_machine(),
            _ => {}
        }

        match self.current_tab {
            Tab::Machine => {
                if matches!(key.code, KeyCode::Enter | KeyCode::Char(' ')) {
                    self.resume_machine();
                }
            }
            Tab::Files => {
                let current = self.file_explorer.current();

                if key.code == KeyCode::Enter && current.path.is_file() {
                    let path = current.path.clone();

                    tracing::info!("Loading {}", path.display());
                    self.core.load_roms(vec![path]);
                } else if let Err(err) = self.file_explorer.handle(event) {
                    tracing::error!("{}: {}", t!("files.cannot_navigate"), err);
                }
            }
            Tab::Log => self.scroll_log(key.code),
            Tab::Debug => {}
        }
    }

    fn scroll_log(&mut self, code: KeyCode) {
        let last_index = self.log.len().saturating_sub(1);
        let selected = self.log_state.selected().unwrap_or(last_index);

        let selected = match code {
            KeyCode::Up | KeyCode::Char('k') => selected.saturating_sub(1),
            KeyCode::Down | KeyCode::Char('j') => selected.saturating_add(1),
            KeyCode::PageUp => selected.saturating_sub(10),
            KeyCode::PageDown => selected.saturating_add(10),
            KeyCode::Home | KeyCode::Char('g') => 0,
            KeyCode::End | KeyCode::Char('G') => last_index,
            _ => return,
        }
        .min(last_index);

        self.follow_log = selected == last_index;
        self.log_state.select(Some(selected));
    }

    fn switch_tab(&mut self, tab: Tab) {
        if self.current_tab != tab {
            self.current_tab = tab;
            self.clear_requested = true;
            self.redraw_requested = true;
        }
    }

    fn resume_machine(&mut self) {
        if self.core.machine().is_none() {
            return;
        }

        self.switch_tab(Tab::Machine);
        self.core.resume_machine();
        self.machine_has_focus = true;
        self.redraw_requested = true;
    }

    fn pause_machine(&mut self) {
        self.core.pause_machine();
        self.machine_has_focus = false;
        self.redraw_requested = true;
    }

    fn machine_is_running(&self) -> bool {
        self.core
            .controller()
            .is_some_and(|controller| !controller.get_paused())
    }

    fn handle_core_events(&mut self) {
        for notification in self.notifications.drain() {
            if self.log.len() == MAX_LOG_ENTRIES {
                self.log.pop_front();
            }

            self.log.push_back(LogEntry {
                notification,
                received: Instant::now(),
            });
            self.redraw_requested = true;
        }

        for event in self.core.drain_events() {
            match event {
                FrontendEvent::MenuToggleRequested => {
                    if self.machine_has_focus {
                        self.pause_machine();
                    } else {
                        self.resume_machine();
                    }
                }
            }
        }
    }

    pub fn draw(&mut self, frame: &mut Frame) {
        let [tab_area, body_area, status_area] = Layout::vertical([
            Constraint::Length(1),
            Constraint::Min(0),
            Constraint::Length(1),
        ])
        .areas(frame.area());

        let tabs = Tabs::new(Tab::iter().map(Tab::title))
            .select(self.current_tab.index())
            .highlight_style(Style::new().reversed());

        frame.render_widget(tabs, tab_area);

        match self.current_tab {
            Tab::Machine => self.draw_machine(frame, body_area),
            Tab::Files => frame.render_widget_ref(self.file_explorer.widget(), body_area),
            Tab::Log => self.draw_log(frame, body_area),
            Tab::Debug => self.draw_debug(frame, body_area),
        }

        self.draw_status(frame, status_area);
    }

    fn draw_machine(&mut self, frame: &mut Frame, area: Rect) {
        let is_running = self.machine_is_running();

        match self.core.machine() {
            Some(machine) => {
                self.machine_view
                    .draw(frame, area, &self.picker, machine, is_running);
            }
            None => {
                let message = Paragraph::new(t!("machine.none").to_string())
                    .centered()
                    .block(Block::bordered());
                frame.render_widget(message, area);
            }
        }
    }

    fn draw_log(&mut self, frame: &mut Frame, area: Rect) {
        let items = self.log.iter().map(|entry| {
            let notification = &entry.notification;

            ListItem::new(Line::from(vec![
                Span::styled(
                    format!("{:>5} ", notification.level),
                    level_style(notification.level),
                ),
                Span::raw(notification.message.as_str()),
            ]))
        });

        if self.follow_log {
            self.log_state.select(self.log.len().checked_sub(1));
        }

        let list = List::new(items)
            .block(Block::bordered().title(Tab::Log.title()))
            .highlight_style(Style::new().reversed());

        frame.render_stateful_widget(list, area, &mut self.log_state);
    }

    fn draw_debug(&self, frame: &mut Frame, area: Rect) {
        let block = Block::bordered().title(Tab::Debug.title());

        let Some(controller) = self.core.controller() else {
            frame.render_widget(
                Paragraph::new(t!("machine.none").to_string()).block(block),
                area,
            );
            return;
        };

        let state = controller.get_state_snapshot();
        let rows = [
            (
                t!("simulation_controller.execution_timeslice"),
                format!("{:?}", Duration::from_secs_f32(state.execution_timeslice)),
            ),
            (
                t!("simulation_controller.target_timeslice"),
                format!("{:?}", Duration::from_secs_f32(state.target_timeslice)),
            ),
            (
                t!("simulation_controller.hardware_speed"),
                format!("{:.1}%", state.hardware_speed_ema * 100.0),
            ),
            (
                t!("simulation_controller.jitter_ratio"),
                format!("{:.1}%", state.jitter_ratio * 100.0),
            ),
        ]
        .map(|(label, value)| Row::new([label.to_string(), value]));

        let table = Table::new(rows, [Constraint::Percentage(50), Constraint::Fill(1)])
            .block(block)
            .row_highlight_style(Style::new().reversed());

        frame.render_widget(table, area);
    }

    fn draw_status(&self, frame: &mut Frame, area: Rect) {
        let hint = if self.machine_has_focus {
            t!("hint.machine_focused")
        } else if self.core.machine().is_some() {
            t!("hint.machine_paused")
        } else {
            t!("hint.no_machine")
        };

        let [hint_area, notification_area] =
            Layout::horizontal([Constraint::Fill(1), Constraint::Fill(1)]).areas(area);

        frame.render_widget(Paragraph::new(hint.to_string()).dark_gray(), hint_area);

        if let Some(entry) = self.log.back()
            && entry.received.elapsed() < STATUS_NOTIFICATION_DURATION
        {
            let notification = Paragraph::new(entry.notification.message.as_str())
                .style(level_style(entry.notification.level))
                .right_aligned();

            frame.render_widget(notification, notification_area);
        }
    }
}

#[inline]
fn level_style(level: Level) -> Style {
    let color = match level {
        Level::ERROR => Color::Red,
        Level::WARN => Color::Yellow,
        Level::INFO => Color::Green,
        Level::DEBUG => Color::Blue,
        Level::TRACE => Color::DarkGray,
    };

    Style::new().fg(color)
}
