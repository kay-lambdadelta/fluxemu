use std::{f64::consts::TAU, time::Duration};

use egui::{
    Color32, Context, FontDefinitions, FontFamily, Id, LayerId, Order, Rect, Response, Sense,
    Stroke, StrokeKind, TextStyle,
};
use nalgebra::Vector2;
use palette::Srgba;

const FOCUS_OUTLINE_WIDTH: f32 = 3.0;
const FOCUS_PULSE_PERIOD_SECONDS: f64 = 2.0;
const FOCUS_PULSE_REPAINT_INTERVAL: Duration = Duration::from_millis(33);

#[inline]
pub fn allocate_fill_aspect(
    ui: &mut egui::Ui,
    min: Vector2<f32>,
    id: Id,
    sense: Sense,
) -> (Rect, Response) {
    let available = ui.available_size();
    let pixels_per_point = ui.ctx().pixels_per_point();

    let fit = |avail: f32, min| {
        if avail.is_finite() && min > 0.0 {
            avail / min
        } else {
            f32::INFINITY
        }
    };

    let mut scale = fit(available.x, min.x).min(fit(available.y, min.y));
    if !scale.is_finite() {
        scale = 1.0;
    }

    let size = egui::vec2(min.x * scale, min.y * scale);

    let container = egui::vec2(
        if available.x.is_finite() {
            available.x.max(size.x)
        } else {
            size.x
        },
        if available.y.is_finite() {
            available.y.max(size.y)
        } else {
            size.y
        },
    );

    let (_, outer) = ui.allocate_space(container);

    let center = outer.center();
    let center = egui::pos2(
        (center.x * pixels_per_point).round() / pixels_per_point,
        (center.y * pixels_per_point).round() / pixels_per_point,
    );

    let rect = Rect::from_center_size(center, size);
    let response = ui.interact(rect, id, sense);

    (rect, response)
}

#[inline]
pub fn setup_egui_context(font_definitions: FontDefinitions) -> Context {
    let egui_context = egui::Context::default();

    egui_context.global_style_mut(|style| {
        style.text_styles.insert(
            TextStyle::Body,
            egui::FontId::new(18.0, FontFamily::Proportional),
        );
        style.text_styles.insert(
            TextStyle::Button,
            egui::FontId::new(20.0, FontFamily::Proportional),
        );
        style.text_styles.insert(
            TextStyle::Heading,
            egui::FontId::new(24.0, FontFamily::Proportional),
        );
    });

    egui_context.set_fonts(font_definitions);

    egui_context.options_mut(|options| {
        options.reduce_texture_memory = true;
    });

    egui_context
}

#[inline]
pub fn to_egui_color(color: impl Into<Srgba<u8>>) -> Color32 {
    let color = color.into();

    Color32::from_rgba_unmultiplied(color.red, color.green, color.blue, color.alpha)
}

pub fn paint_focus_outline(context: &Context, excluded: Id) {
    let Some(response) = context
        .memory(|memory| memory.focused())
        .filter(|focused| *focused != excluded)
        .and_then(|focused| context.read_response(focused))
    else {
        return;
    };

    let time = context.input(|input| input.time);
    let wave = (time * TAU / FOCUS_PULSE_PERIOD_SECONDS).sin() as f32;
    let style = context.global_style();
    let visuals = &style.visuals;

    context
        .layer_painter(LayerId::new(Order::Foreground, Id::unique("focus_outline")))
        .rect_stroke(
            response.interact_rect,
            visuals.widgets.inactive.corner_radius,
            Stroke::new(
                FOCUS_OUTLINE_WIDTH,
                visuals
                    .selection
                    .stroke
                    .color
                    .gamma_multiply(0.6 + 0.4 * wave),
            ),
            StrokeKind::Inside,
        );

    context.request_repaint_after(FOCUS_PULSE_REPAINT_INTERVAL);
}
