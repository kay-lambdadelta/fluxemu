use std::time::Duration;

use egui::{Align2, Order, Ui};
use egui_toast::{Toast, ToastKind, ToastOptions, ToastStyle, Toasts};
use fluxemu_frontend::notification::Notification;
use tracing::Level;

pub struct ToastManager {
    toasts: Toasts,
}

impl Default for ToastManager {
    fn default() -> Self {
        Self {
            toasts: Toasts::default()
                .anchor(Align2::RIGHT_TOP, [-10.0, -10.0])
                .order(Order::Foreground),
        }
    }
}

impl ToastManager {
    pub fn toast(&mut self, notification: Notification) {
        let kind = match notification.level {
            Level::ERROR => ToastKind::Error,
            Level::WARN => ToastKind::Warning,
            _ => ToastKind::Info,
        };

        self.toasts.add(Toast {
            kind,
            text: notification.message.into(),
            options: ToastOptions::default()
                .duration(Some(Duration::from_secs(5)))
                .show_icon(true)
                .show_progress(true),
            style: ToastStyle::default(),
        });
    }

    pub fn show(&mut self, ui: &mut Ui) {
        self.toasts.show(ui);
    }
}
