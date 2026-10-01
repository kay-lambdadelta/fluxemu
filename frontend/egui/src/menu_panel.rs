use egui::{Button, Frame, Panel, Response, RichText, Ui, Widget};
use strum::{AsRefStr, EnumIter, IntoEnumIterator};

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Hash, EnumIter, AsRefStr)]
pub enum TabId {
    #[default]
    Machine,
    Library,
    FileBrowser,
    Settings,
    Log,
    Controller,
    Debug,
    About,
}

impl TabId {
    pub fn icon(self) -> &'static str {
        match self {
            Self::Machine => "▶️",
            Self::FileBrowser => "📁",
            Self::Library => "📚",
            Self::Settings => "⚙️",
            Self::Log => "📝",
            Self::Controller => "🎮",
            Self::Debug => "🐞",
            Self::About => "ℹ️",
        }
    }
}

pub struct MenuPanel {
    pub current_tab: TabId,
    pub is_expanded: bool,
}

impl Default for MenuPanel {
    fn default() -> Self {
        Self {
            current_tab: TabId::default(),
            is_expanded: true,
        }
    }
}

impl Widget for &mut MenuPanel {
    fn ui(self, ui: &mut Ui) -> Response {
        Panel::top("menu_selection")
            .show_collapsible(ui, &mut self.is_expanded, |ui| {
                Frame::default().inner_margin(8.0).show(ui, |ui| {
                    let button_size = egui::vec2(48.0, 48.0);
                    let count = TabId::iter().count() as f32;

                    let spacing = ui.spacing().item_spacing.x;
                    let total_width = count * button_size.x + (count - 1.0) * spacing;

                    ui.horizontal(|ui| {
                        ui.add_space(((ui.available_width() - total_width) / 2.0).max(0.0));

                        for tab in TabId::iter() {
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
