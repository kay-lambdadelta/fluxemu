use std::{
    borrow::Cow,
    sync::mpsc::{Receiver, RecvTimeoutError, Sender, channel},
    time::Duration,
};

use crossterm::event::Event;
use fluxemu_input::{InputId, InputState, physical::PhysicalInputDeviceId};

#[derive(Debug)]
pub(crate) enum Message {
    Terminal(Event),
    TerminalError(std::io::Error),
    GamepadConnected {
        id: PhysicalInputDeviceId,
        name: Cow<'static, str>,
    },
    GamepadDisconnected {
        id: PhysicalInputDeviceId,
    },
    Input {
        id: PhysicalInputDeviceId,
        input_id: InputId,
        state: InputState,
    },
}

#[derive(Debug, Clone)]
pub struct ExternalInputSender(Sender<Message>);

impl ExternalInputSender {
    pub fn register_gamepad(&self, id: PhysicalInputDeviceId, name: impl Into<Cow<'static, str>>) {
        self.send(Message::GamepadConnected {
            id,
            name: name.into(),
        });
    }

    pub fn unregister_gamepad(&self, id: PhysicalInputDeviceId) {
        self.send(Message::GamepadDisconnected { id });
    }

    pub fn insert_input(&self, id: PhysicalInputDeviceId, input_id: InputId, state: InputState) {
        self.send(Message::Input {
            id,
            input_id,
            state,
        });
    }

    pub(crate) fn send(&self, message: Message) {
        let _ = self.0.send(message);
    }
}

#[derive(Debug)]
pub struct ExternalInputReceiver {
    sender: ExternalInputSender,
    receiver: Receiver<Message>,
}

impl ExternalInputReceiver {
    pub(crate) fn sender(&self) -> ExternalInputSender {
        self.sender.clone()
    }

    pub(crate) fn recv_timeout(&self, timeout: Duration) -> Option<Message> {
        match self.receiver.recv_timeout(timeout) {
            Ok(message) => Some(message),
            Err(RecvTimeoutError::Timeout) => None,
            Err(RecvTimeoutError::Disconnected) => unreachable!(),
        }
    }

    pub(crate) fn try_recv(&self) -> Option<Message> {
        self.receiver.try_recv().ok()
    }
}

pub fn external_input_channel() -> (ExternalInputSender, ExternalInputReceiver) {
    let (sender, receiver) = channel();
    let sender = ExternalInputSender(sender);

    (sender.clone(), ExternalInputReceiver { sender, receiver })
}
