use std::{
    collections::HashMap,
    sync::{Arc, Weak},
    time::{Duration, Instant, SystemTime},
};

use egui::{
    Align, Button, Color32, ColorImage, CornerRadius, Image, Layout, Rect, ScrollArea, Sense,
    TextEdit, TextStyle, TextWrapMode, TextureHandle, TextureOptions, Ui, vec2,
};
use fluxemu_frontend::{Platform, SlotInfo, SlotKind, SlotMetadata};
use fluxemu_runtime::machine::Machine;
use jiff::{Timestamp, tz::TimeZone};
use nalgebra::Vector2;
use palette::{WithAlpha, named::GREEN};
use rust_i18n::t;

use crate::{
    Frontend,
    utils::{allocate_fill_aspect, to_egui_color},
};

const REFRESH_INTERVAL: Duration = Duration::from_millis(500);
const THUMBNAIL_SIZE: Vector2<f32> = Vector2::new(96.0, 90.0);
const DETAIL_IMAGE_WIDTH_FRACTION: f32 = 0.6;

#[derive(Debug)]
struct Detail {
    slot: u8,
    name: String,
    notes: String,
    multiline_name: bool,
    machine: Weak<Machine>,
}

#[derive(Default)]
pub struct SlotList {
    slots: Vec<SlotInfo>,
    last_refresh: Option<Instant>,
    thumbnails: HashMap<(u8, SystemTime), Option<TextureHandle>>,
    detail: Option<Detail>,
    pending_delete: Option<u8>,
}

enum Action {
    New,
    Store(u8),
    Load(u8),
    Activate(u8),
    UseSave(u8),
    Delete(u8),
    OpenDetail(u8),
    SaveDetail,
    CloseDetail,
    UndoLoad,
    UndoStore,
}

fn format_time(time: SystemTime) -> String {
    Timestamp::try_from(time).map_or_else(
        |_| String::new(),
        |timestamp| {
            timestamp
                .to_zoned(TimeZone::system())
                .strftime("%Y-%m-%d %H:%M")
                .to_string()
        },
    )
}

fn slot_label(info: &SlotInfo) -> String {
    let name = info
        .metadata
        .display_name()
        .map_or_else(|| t!("slots.unnamed").into_owned(), str::to_owned);

    format!(
        "{name}\n{} {} - {}",
        t!("slots.slot"),
        info.slot,
        format_time(info.modified)
    )
}

fn optional_text(text: &str) -> Option<String> {
    (!text.is_empty()).then(|| text.to_owned())
}

impl<P: Platform> Frontend<P> {
    fn slot_list(&mut self, kind: SlotKind) -> &mut SlotList {
        match kind {
            SlotKind::Save => &mut self.save_slots,
            SlotKind::Snapshot => &mut self.snapshot_slots,
        }
    }

    fn refresh_slots(&mut self, kind: SlotKind) {
        let is_stale = self
            .slot_list(kind)
            .last_refresh
            .is_none_or(|last_refresh| last_refresh.elapsed() >= REFRESH_INTERVAL);

        if is_stale {
            let slots = self.core.list_slots(kind);
            let list = self.slot_list(kind);

            list.slots = slots;
            list.last_refresh = Some(Instant::now());
        }
    }

    fn close_stale_detail(&mut self, kind: SlotKind) {
        let machine = self.core.machine().map(Arc::as_ptr);
        let list = self.slot_list(kind);

        if list
            .detail
            .as_ref()
            .is_some_and(|detail| Some(detail.machine.as_ptr()) != machine)
        {
            list.detail = None;
        }
    }

    fn thumbnail(&mut self, ui: &Ui, info: &SlotInfo) -> Option<TextureHandle> {
        let key = (info.slot, info.modified);

        if let Some(thumbnail) = self.snapshot_slots.thumbnails.get(&key) {
            return thumbnail.clone();
        }

        let thumbnail = self.core.slot_screenshot(info.slot).map(|screenshot| {
            let pixels: Vec<_> = screenshot
                .iter_pixels()
                .map(|pixel| {
                    Color32::from_rgba_unmultiplied(pixel.red, pixel.green, pixel.blue, pixel.alpha)
                })
                .collect();

            ui.ctx().load_texture(
                format!("snapshot-{}", info.slot),
                ColorImage::new([screenshot.width(), screenshot.height()], pixels),
                TextureOptions::NEAREST,
            )
        });

        self.snapshot_slots
            .thumbnails
            .insert(key, thumbnail.clone());

        thumbnail
    }

    fn show_large_image(&mut self, ui: &mut Ui, info: &SlotInfo) {
        match self.thumbnail(ui, info) {
            Some(image) => {
                let size = image.size();
                let (rect, _) = allocate_fill_aspect(
                    ui,
                    Vector2::new(size[0] as f32, size[1] as f32),
                    ui.scope_id().with("slot_image"),
                    Sense::hover(),
                );

                Image::new(&image)
                    .fit_to_exact_size(rect.size())
                    .paint_at(ui, rect);
            }
            None => {
                let (rect, _) = ui.allocate_exact_size(ui.available_size(), Sense::hover());

                ui.painter()
                    .rect_filled(rect, CornerRadius::ZERO, ui.visuals().extreme_bg_color);
            }
        }
    }

    fn show_detail_fields(&mut self, ui: &mut Ui, kind: SlotKind) {
        let Some(detail) = &mut self.slot_list(kind).detail else {
            return;
        };

        ui.label(t!("slots.name"));

        let name = if detail.multiline_name {
            TextEdit::multiline(&mut detail.name).desired_rows(2)
        } else {
            TextEdit::singleline(&mut detail.name)
        };

        ui.add(name.font(TextStyle::Heading).desired_width(f32::INFINITY));

        ui.label(t!("slots.notes"));
        ui.add_sized(ui.available_size(), TextEdit::multiline(&mut detail.notes));
    }

    fn show_detail(
        &mut self,
        ui: &mut Ui,
        kind: SlotKind,
        info: &SlotInfo,
        active_slot: u8,
        actions: &mut Vec<Action>,
    ) {
        ui.horizontal_top(|ui| {
            if ui.button(t!("slots.save_changes")).clicked() {
                actions.push(Action::SaveDetail);
            }

            if ui.button(t!("slots.cancel")).clicked() {
                actions.push(Action::CloseDetail);
            }

            self.show_detail_actions(ui, kind, info, info.slot == active_slot, actions);

            ui.label(format!("{} {}", t!("slots.slot"), info.slot));
            ui.weak(format_time(info.modified));
        });

        ui.separator();

        if kind == SlotKind::Snapshot {
            let size = ui.available_size();

            ui.horizontal_top(|ui| {
                ui.allocate_ui_with_layout(
                    vec2(size.x * DETAIL_IMAGE_WIDTH_FRACTION, size.y),
                    Layout::top_down(Align::Center),
                    |ui| self.show_large_image(ui, info),
                );

                ui.vertical(|ui| self.show_detail_fields(ui, kind));
            });
        } else {
            self.show_detail_fields(ui, kind);
        }
    }

    fn show_detail_actions(
        &mut self,
        ui: &mut Ui,
        kind: SlotKind,
        info: &SlotInfo,
        is_active: bool,
        actions: &mut Vec<Action>,
    ) {
        let slot = info.slot;
        let list = self.slot_list(kind);

        if list.pending_delete == Some(slot) {
            ui.label(t!("slots.delete_prompt"));

            if ui.button(t!("slots.delete")).clicked() {
                actions.push(Action::Delete(slot));
            }

            if ui.button(t!("slots.cancel")).clicked() {
                list.pending_delete = None;
            }

            return;
        }

        match kind {
            SlotKind::Snapshot => {
                if ui.button(t!("slots.load")).clicked() {
                    actions.push(Action::Load(slot));
                }

                if ui.button(t!("slots.overwrite")).clicked() {
                    actions.push(Action::Store(slot));
                }

                if !is_active && ui.button(t!("slots.make_active")).clicked() {
                    actions.push(Action::Activate(slot));
                }
            }
            SlotKind::Save => {
                if !is_active && ui.button(t!("slots.use_save")).clicked() {
                    actions.push(Action::UseSave(slot));
                }
            }
        }

        let can_delete = kind == SlotKind::Snapshot || !is_active;

        if ui
            .add_enabled(can_delete, Button::new(t!("slots.delete")))
            .clicked()
        {
            list.pending_delete = Some(slot);
        }
    }

    fn show_slot(
        &mut self,
        ui: &mut Ui,
        kind: SlotKind,
        info: &SlotInfo,
        active_slot: u8,
        actions: &mut Vec<Action>,
    ) {
        let is_active = info.slot == active_slot;
        let label = slot_label(info);

        let thumbnail_size = vec2(THUMBNAIL_SIZE.x, THUMBNAIL_SIZE.y);
        let thumbnail_rect = Rect::from_min_size(ui.cursor().min, thumbnail_size);
        let thumbnail = (kind == SlotKind::Snapshot && ui.is_rect_visible(thumbnail_rect))
            .then(|| self.thumbnail(ui, info))
            .flatten();

        let button = match &thumbnail {
            Some(thumbnail) => Button::image_and_text(
                Image::new(thumbnail).fit_to_exact_size(thumbnail_size),
                label,
            ),
            None => Button::new(label),
        };

        let minimum_height = match kind {
            SlotKind::Snapshot => THUMBNAIL_SIZE.y,
            SlotKind::Save => 0.0,
        };

        let mut button = button
            .wrap_mode(TextWrapMode::Wrap)
            .min_size(vec2(0.0, minimum_height));

        if is_active {
            button = button.fill(to_egui_color(GREEN.with_alpha(u8::MAX)));
        }

        if ui.add(button).clicked() {
            actions.push(Action::OpenDetail(info.slot));
        }
    }

    fn show_slot_toolbar(&mut self, ui: &mut Ui, kind: SlotKind, actions: &mut Vec<Action>) {
        ui.horizontal_top(|ui| {
            let has_free_slot = self.core.next_free_slot(kind).is_some();

            let new_label = match kind {
                SlotKind::Save => t!("slots.new_save"),
                SlotKind::Snapshot => t!("slots.new_snapshot"),
            };

            if ui
                .add_enabled(has_free_slot, Button::new(new_label))
                .clicked()
            {
                actions.push(Action::New);
            }

            if kind == SlotKind::Snapshot {
                let undo_load = Button::new(t!("slots.undo_load"));

                if ui
                    .add_enabled(self.core.can_undo_load_snapshot(), undo_load)
                    .clicked()
                {
                    actions.push(Action::UndoLoad);
                }

                let undo_store = Button::new(t!("slots.undo_store"));

                if ui
                    .add_enabled(self.core.can_undo_store_snapshot(), undo_store)
                    .clicked()
                {
                    actions.push(Action::UndoStore);
                }
            }
        });

        ui.separator();
    }

    fn apply_slot_action(
        &mut self,
        kind: SlotKind,
        action: Action,
        graphics_runtime: &P::GraphicsRuntime,
    ) {
        match action {
            Action::New => {
                if let Some(slot) = self.core.next_free_slot(kind) {
                    match kind {
                        SlotKind::Save => self.core.use_save_slot(slot),
                        SlotKind::Snapshot => {
                            self.core.set_active_snapshot_slot(slot);
                            self.core.store_snapshot(slot, graphics_runtime);
                        }
                    }
                }
            }
            Action::Store(slot) => self.core.store_snapshot(slot, graphics_runtime),
            Action::Load(slot) => self.core.load_snapshot(slot),
            Action::Activate(slot) => self.core.set_active_snapshot_slot(slot),
            Action::UseSave(slot) => self.core.use_save_slot(slot),
            Action::Delete(slot) => {
                self.core.delete_slot(kind, slot);
                self.slot_list(kind).pending_delete = None;
            }
            Action::OpenDetail(slot) => {
                let machine = self.core.machine().map(Arc::downgrade);
                let list = self.slot_list(kind);

                if let (Some(machine), Some(info)) =
                    (machine, list.slots.iter().find(|info| info.slot == slot))
                {
                    let name = info.metadata.name.clone().unwrap_or_default();

                    list.detail = Some(Detail {
                        slot,
                        multiline_name: name.contains('\n'),
                        name,
                        notes: info.metadata.notes.clone().unwrap_or_default(),
                        machine,
                    });
                }
            }
            Action::SaveDetail => {
                if let Some(detail) = self.slot_list(kind).detail.take() {
                    self.core.set_slot_metadata(
                        kind,
                        detail.slot,
                        &SlotMetadata {
                            name: optional_text(&detail.name),
                            notes: optional_text(&detail.notes),
                        },
                    );
                }
            }
            Action::CloseDetail => self.slot_list(kind).detail = None,
            Action::UndoLoad => self.core.undo_load_snapshot(),
            Action::UndoStore => self.core.undo_store_snapshot(),
        }

        self.slot_list(kind).last_refresh = None;
    }

    pub fn handle_slots(
        &mut self,
        ui: &mut Ui,
        kind: SlotKind,
        graphics_runtime: &P::GraphicsRuntime,
    ) {
        self.refresh_slots(kind);

        ui.ctx().request_repaint_after(REFRESH_INTERVAL);

        let mut actions = Vec::new();

        let active_slot = match kind {
            SlotKind::Save => self.core.active_save_slot(),
            SlotKind::Snapshot => self.core.active_snapshot_slot(),
        };

        let slots = self.slot_list(kind).slots.clone();

        self.close_stale_detail(kind);

        let detail_info = self
            .slot_list(kind)
            .detail
            .as_ref()
            .and_then(|detail| slots.iter().find(|info| info.slot == detail.slot));

        if let Some(info) = detail_info {
            self.show_detail(ui, kind, info, active_slot, &mut actions);
        } else {
            self.show_slot_toolbar(ui, kind, &mut actions);

            ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
                ui.with_layout(
                    Layout::top_down(Align::LEFT).with_cross_justify(true),
                    |ui| {
                        if slots.is_empty() {
                            ui.weak(t!("slots.empty"));
                        }

                        for info in &slots {
                            self.show_slot(ui, kind, info, active_slot, &mut actions);
                        }
                    },
                );
            });
        }

        for action in actions {
            self.apply_slot_action(kind, action, graphics_runtime);
        }
    }
}
