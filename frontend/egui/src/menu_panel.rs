use egui::{Button, Frame, Panel, Response, RichText, Ui, Widget};
use strum::{AsRefStr, EnumIter, IntoEnumIterator};

use crate::icon;

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Hash, EnumIter, AsRefStr)]
pub enum TabId {
    Machine,
    #[default]
    Library,
    Snapshots,
    Saves,
    FileBrowser,
    Settings,
}

impl TabId {
    fn needs_machine(self) -> bool {
        matches!(self, Self::Snapshots | Self::Saves | Self::Machine)
    }

    pub fn icon(self) -> &'static str {
        match self {
            Self::Machine => icon::PLAY,
            Self::Library => icon::LIBRARY,
            Self::Snapshots => icon::CAMERA,
            Self::Saves => icon::SAVE,
            Self::FileBrowser => icon::FOLDER_OPEN,
            Self::Settings => icon::SETTINGS,
        }
    }
}

pub struct MenuPanel {
    pub current_tab: TabId,
    pub is_expanded: bool,
    pub machine_running: bool,
}

impl Default for MenuPanel {
    fn default() -> Self {
        Self {
            current_tab: TabId::default(),
            is_expanded: true,
            machine_running: false,
        }
    }
}

impl Widget for &mut MenuPanel {
    fn ui(self, ui: &mut Ui) -> Response {
        if self.current_tab.needs_machine() && !self.machine_running {
            self.current_tab = TabId::default();
        }

        let machine_running = self.machine_running;
        let visible_tabs =
            move || TabId::iter().filter(move |tab| machine_running || !tab.needs_machine());

        Panel::top("menu_selection")
            .show_collapsible(ui, &mut self.is_expanded, |ui| {
                Frame::default()
                    .inner_margin(ui.spacing().menu_margin)
                    .show(ui, |ui| {
                        let button_size = egui::vec2(48.0, 48.0);
                        let count = visible_tabs().count() as f32;

                        let spacing = ui.spacing().item_spacing.x;
                        let total_width = count * button_size.x + (count - 1.0) * spacing;

                        ui.horizontal(|ui| {
                            ui.add_space(((ui.available_width() - total_width) / 2.0).max(0.0));

                            for tab in visible_tabs() {
                                let mut icon = RichText::new(tab.icon()).size(32.0);

                                if self.current_tab == tab {
                                    icon = icon.strong();
                                }

                                let response = ui.add_sized(button_size, Button::new(icon));

                                if response.clicked() {
                                    self.current_tab = tab;
                                }

                                // If nothing is focused, focus on the default button
                                if tab == TabId::default()
                                    && ui.memory(|memory| memory.focused().is_none())
                                {
                                    response.request_focus();
                                }
                            }
                        });
                    });
            })
            .map(|inner| inner.response)
            .unwrap_or_else(|| ui.response())
    }
}
