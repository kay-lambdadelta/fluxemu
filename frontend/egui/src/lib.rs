//! Generic multiplatform frontend implementation for fluxemu

mod file_browser;
mod input;
mod marquee;
mod menu_panel;
pub mod rendering;
mod settings;
mod simulation_controller;
mod utils;

use std::sync::Arc;

use egui::{
    Align, CentralPanel, FontDefinitions, Frame, FullOutput, Id, Layout, RawInput, Rect, Sense,
    pos2, vec2,
};
use fluxemu_frontend::{FrontendCore, FrontendEvent, Platform, notification::NotificationReceiver};
use fluxemu_graphics::api::{Framebuffer, GraphicsApi};
use fluxemu_runtime::machine::{Machine, builder::SealedMachineBuilder};
use nalgebra::Vector2;
use palette::{
    WithAlpha,
    named::{BLACK, WHITE},
};

use crate::{
    file_browser::{FileBrowser, FileBrowserState},
    input::translator::EguiInputTranslator,
    marquee::NotificationMarquee,
    menu_panel::{MenuPanel, TabId},
    rendering::EguiCapableGraphicsRuntime,
    utils::{allocate_fill_aspect, setup_egui_context, to_egui_color},
};

rust_i18n::i18n!("locales", fallback = "en");

pub struct Frontend<P: Platform> {
    core: FrontendCore<P>,
    notifications: NotificationReceiver,
    egui_context: egui::Context,
    file_browser_state: FileBrowserState,
    menu_panel: MenuPanel,
    simulation_controller_ui_state: simulation_controller::State,
    notification_marquee: NotificationMarquee,
    font_definitions: FontDefinitions,
    egui_input_translator: EguiInputTranslator,
    /// ID of area where framebuffers will be drawn
    machine_framebuffer_display_area: Id,
}

impl<P: Platform> Frontend<P> {
    pub fn new(
        core: FrontendCore<P>,
        notifications: NotificationReceiver,
        font_definitions: FontDefinitions,
    ) -> Self {
        let file_browser_state =
            FileBrowserState::new(core.environment().file_browser_home_directory.clone());

        Self {
            core,
            notifications,
            egui_context: setup_egui_context(font_definitions.clone()),
            file_browser_state,
            menu_panel: MenuPanel::default(),
            simulation_controller_ui_state: simulation_controller::State::default(),
            notification_marquee: NotificationMarquee::default(),
            font_definitions,
            egui_input_translator: EguiInputTranslator::default(),
            machine_framebuffer_display_area: Id::unique(format!(
                "{}/machine_framebuffer_display_area",
                env!("CARGO_CRATE_NAME")
            )),
        }
    }

    pub fn egui_context(&self) -> &egui::Context {
        &self.egui_context
    }

    pub fn machine(&self) -> Option<&Arc<Machine>> {
        self.core.machine()
    }

    pub fn maybe_reset_graphics_to_meet_machine_requirements(
        &mut self,
        callback: impl FnOnce(
            &egui::Context,
            &SealedMachineBuilder<P>,
        ) -> <P::GraphicsApi as GraphicsApi>::InitializationData,
    ) {
        if !self.core.has_pending_machine() {
            return;
        }

        self.egui_context = setup_egui_context(self.font_definitions.clone());
        let egui_context = self.egui_context.clone();

        self.core.start_pending_machine(|sealed_machine_builder| {
            callback(&egui_context, sealed_machine_builder)
        });

        self.simulation_controller_ui_state = simulation_controller::State::default();
        self.set_machine_focus(true);
        self.menu_panel.is_expanded = false;
        self.menu_panel.current_tab = TabId::Machine;
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

        self.core.poll();
        self.handle_core_events();

        let full_output = self.egui_context.clone().run_ui(external_input, |ui| {
            self.notification_marquee.show(ui);

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
                                core: &mut self.core,
                            });
                        }
                        TabId::Settings => {
                            self.handle_settings(ui);
                        }
                        TabId::Log => {}
                        TabId::Controller => {}
                        TabId::Debug => {
                            if let Some(controller) = self.core.controller() {
                                let ui_state = &mut self.simulation_controller_ui_state;

                                ui_state.update(controller);
                                ui.add(ui_state);
                            }
                        }
                        TabId::About => {}
                    });
                });
            });
        });

        // Pick up anything the UI caused this frame
        self.handle_core_events();

        full_output
    }

    fn handle_core_events(&mut self) {
        for notification in self.notifications.drain() {
            self.notification_marquee.push(notification);
            self.egui_context.request_repaint();
        }

        for event in self.core.drain_events() {
            match event {
                FrontendEvent::MenuToggleRequested => {
                    if self.core.machine().is_none()
                        || self.menu_panel.current_tab != TabId::Machine
                    {
                        continue;
                    }

                    if self.menu_panel.is_expanded {
                        self.core.resume_machine();
                        self.set_machine_focus(true);
                        self.menu_panel.is_expanded = false;
                    } else {
                        self.core.pause_machine();
                        self.set_machine_focus(false);
                        self.menu_panel.is_expanded = true;
                    }
                }
            }
        }
    }

    fn draw_machine_framebuffers(
        &mut self,
        ui: &mut egui::Ui,
        graphics_runtime: &mut P::GraphicsRuntime,
    ) where
        P::GraphicsRuntime: EguiCapableGraphicsRuntime,
    {
        if let Some(machine) = self.core.machine()
            && let Some(controller) = self.core.controller()
        {
            let runtime = machine.enter_runtime();
            let framebuffer = runtime.list_framebuffers().next().unwrap();

            let callback = runtime
                .read_framebuffer::<P::GraphicsApi, _>(
                    framebuffer,
                    runtime.safe_advance_timestamp(),
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
