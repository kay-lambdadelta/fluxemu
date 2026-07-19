use std::{borrow::Cow, time::Duration};

use egui::{Align2, Order, Ui};
use egui_toast::{Toast, ToastKind, ToastOptions, ToastStyle, Toasts};

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
    pub fn toast(&mut self, kind: ToastKind, message: impl Into<Cow<'static, str>>) {
        let message = message.into();

        match kind {
            ToastKind::Info => {
                tracing::info!("{}", message);
            }
            ToastKind::Warning => {
                tracing::warn!("{}", message);
            }
            ToastKind::Error => {
                tracing::error!("{}", message);
            }
            ToastKind::Success => {
                tracing::info!("{}", message);
            }
            ToastKind::Custom(_) => {
                tracing::info!("{}", message);
            }
        };

        self.toasts.add(Toast {
            kind: ToastKind::Error,
            text: message.into(),
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
