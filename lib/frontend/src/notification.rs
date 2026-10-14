use std::{
    fmt::Debug,
    sync::mpsc::{Receiver, Sender, channel},
};

use tracing::{
    Event, Level, Subscriber,
    field::{Field, Visit},
};
use tracing_subscriber::{Layer, layer::Context};

#[derive(Debug, Clone)]
pub struct Notification {
    pub level: Level,
    pub message: String,
}

pub struct NotificationLayer {
    sender: Sender<Notification>,
}

pub struct NotificationReceiver {
    receiver: Receiver<Notification>,
}

impl NotificationReceiver {
    pub fn drain(&self) -> impl Iterator<Item = Notification> + '_ {
        self.receiver.try_iter()
    }
}

pub fn notification_channel() -> (NotificationLayer, NotificationReceiver) {
    let (sender, receiver) = channel();

    (
        NotificationLayer { sender },
        NotificationReceiver { receiver },
    )
}

impl<S: Subscriber> Layer<S> for NotificationLayer {
    fn on_event(&self, event: &Event<'_>, _context: Context<'_, S>) {
        let metadata = event.metadata();

        let mut visitor = MessageVisitor::default();
        event.record(&mut visitor);

        // The frontend going away is not our problem
        let _ = self.sender.send(Notification {
            level: *metadata.level(),
            message: visitor.message,
        });
    }
}

#[derive(Default)]
struct MessageVisitor {
    message: String,
}

impl Visit for MessageVisitor {
    fn record_str(&mut self, field: &Field, value: &str) {
        if field.name() == "message" {
            self.message = value.to_string();
        }
    }

    fn record_debug(&mut self, field: &Field, value: &dyn Debug) {
        if field.name() == "message" {
            self.message = format!("{:?}", value);
        }
    }
}
