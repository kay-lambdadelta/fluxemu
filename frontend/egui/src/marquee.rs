use egui::{
    Color32, CornerRadius, Frame, Panel, Rect, Sense, Stroke, TextStyle, Ui, Visuals, pos2, vec2,
};
use fluxemu_frontend::notification::Notification;
use ringbuffer::{AllocRingBuffer, RingBuffer};
use tracing::Level;

use crate::icon;

const SCROLL_SPEED: f32 = 150.0;
const MESSAGE_SPACING: f32 = 64.0;
const SEPARATOR_HEIGHT: f32 = 0.7;
const BACKGROUND_OPACITY: f32 = 0.15;
const VERTICAL_PADDING: f32 = 2.0;

fn level_icon(level: Level) -> &'static str {
    match level {
        Level::ERROR => icon::CIRCLE_X,
        Level::WARN => icon::TRIANGLE_ALERT,
        Level::INFO => icon::INFO,
        Level::DEBUG => icon::BUG,
        Level::TRACE => icon::FOOTPRINTS,
    }
}

fn level_color(visuals: &Visuals, level: Level) -> Color32 {
    match level {
        Level::ERROR => visuals.error_fg_color,
        Level::WARN => visuals.warn_fg_color,
        // This is probably a nice shade of blue most of the time
        Level::INFO => visuals.hyperlink_color,
        Level::DEBUG | Level::TRACE => visuals.weak_text_color(),
    }
}

struct Entry {
    notification: Notification,
    position: Option<f32>,
    width: f32,
}

pub struct NotificationMarquee {
    entries: AllocRingBuffer<Entry>,
}

impl Default for NotificationMarquee {
    fn default() -> Self {
        Self {
            entries: AllocRingBuffer::new(128),
        }
    }
}

impl NotificationMarquee {
    pub fn push(&mut self, notification: Notification) {
        self.entries.enqueue(Entry {
            notification,
            position: None,
            width: 0.0,
        });
    }

    pub fn show(&mut self, ui: &mut Ui) {
        let mut is_expanded = !self.entries.is_empty();

        Panel::top("notification_marquee")
            .resizable(false)
            .frame(Frame::NONE.fill(ui.visuals().panel_fill))
            .show_collapsible(ui, &mut is_expanded, |ui| {
                Frame::NONE
                    .fill(ui.visuals().extreme_bg_color)
                    .show(ui, |ui| {
                        let font = TextStyle::Body.resolve(ui.style());
                        let label_font = TextStyle::Small.resolve(ui.style());
                        let label_height = ui.text_style_height(&TextStyle::Small);
                        let height = label_height
                            + ui.text_style_height(&TextStyle::Body)
                            + VERTICAL_PADDING * 2.0;

                        let (rect, _) = ui.allocate_exact_size(
                            vec2(ui.available_width(), height),
                            Sense::hover(),
                        );
                        let painter = ui.painter_at(rect);

                        let scroll_distance = SCROLL_SPEED * ui.input(|input| input.stable_dt);

                        let separator_stroke =
                            Stroke::new(2.0, ui.visuals().widgets.noninteractive.bg_stroke.color);
                        let separator_span = rect
                            .y_range()
                            .shrink(height * (1.0 - SEPARATOR_HEIGHT) / 2.0);

                        let text_color = ui.visuals().text_color();
                        let mut previous_end = None;

                        for entry in &mut self.entries {
                            let level = entry.notification.level;
                            let label_color = level_color(ui.visuals(), level);

                            let galley = painter.layout_no_wrap(
                                entry.notification.message.clone(),
                                font.clone(),
                                text_color,
                            );
                            let label_galley = painter.layout_no_wrap(
                                format!("{} {}", level_icon(level), level),
                                label_font.clone(),
                                label_color,
                            );
                            entry.width = galley.size().x.max(label_galley.size().x);

                            let position = match &mut entry.position {
                                Some(position) => {
                                    *position -= scroll_distance;
                                    *position
                                }
                                unplaced => {
                                    let earliest = previous_end
                                        .map_or(f32::NEG_INFINITY, |end| end + MESSAGE_SPACING);

                                    *unplaced.insert(rect.width().max(earliest))
                                }
                            };

                            let left = rect.left() + position;

                            painter.rect_filled(
                                Rect::from_x_y_ranges(
                                    (left - MESSAGE_SPACING / 2.0)
                                        ..=(left + entry.width + MESSAGE_SPACING / 2.0),
                                    rect.y_range(),
                                ),
                                CornerRadius::ZERO,
                                label_color.gamma_multiply(BACKGROUND_OPACITY),
                            );

                            if let Some(end) = previous_end {
                                painter.vline(
                                    rect.left() + end + MESSAGE_SPACING / 2.0,
                                    separator_span,
                                    separator_stroke,
                                );
                            }
                            previous_end = Some(position + entry.width);

                            let top = rect.top() + VERTICAL_PADDING;

                            painter.galley(pos2(left, top), label_galley, label_color);
                            painter.galley(pos2(left, top + label_height), galley, text_color);
                        }

                        ui.ctx().request_repaint();
                    });
            });

        while self.entries.front().is_some_and(|entry| {
            entry
                .position
                // Wait for the trailing separator to leave too
                .is_some_and(|position| position + entry.width + MESSAGE_SPACING < 0.0)
        }) {
            self.entries.dequeue();
        }
    }
}
