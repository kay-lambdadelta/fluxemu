use std::{
    error::Error,
    ops::{Deref, DerefMut},
    sync::{
        Arc, Condvar, Mutex,
        mpsc::{self, Receiver},
    },
};

use ::input::Libinput;
use libseat::{Seat, SeatEvent};

pub type SharedSeat = Arc<Mutex<SendSeat>>;

pub struct SendSeat(Seat);

// SAFETY: libseat has no thread affinity, and SharedSeat serializes all access through its mutex
unsafe impl Send for SendSeat {}

impl Deref for SendSeat {
    type Target = Seat;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for SendSeat {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

pub fn open_seat() -> Result<(SharedSeat, Receiver<SeatEvent>), Box<dyn Error>> {
    let (event_sender, seat_events) = mpsc::channel();

    let mut seat = Seat::open(move |_, event| {
        event_sender.send(event).unwrap();
    })?;

    while !seat_events
        .try_iter()
        .any(|event| matches!(event, SeatEvent::Enable))
    {
        seat.dispatch(-1)?;
    }

    Ok((Arc::new(Mutex::new(SendSeat(seat))), seat_events))
}

pub fn handle_seat_events(
    seat: &SharedSeat,
    seat_events: &Receiver<SeatEvent>,
    libinput: &mut Libinput,
    presentation_gate: &PresentationGate,
) {
    seat.lock().unwrap().dispatch(0).unwrap();

    for event in seat_events.try_iter() {
        match event {
            SeatEvent::Enable => {
                libinput.resume().unwrap();
                presentation_gate.resume();
            }
            SeatEvent::Disable => {
                presentation_gate.pause();
                libinput.suspend();
                seat.lock().unwrap().disable().unwrap();
            }
        }
    }
}

#[derive(Debug, Default)]
struct PresentationState {
    paused: bool,
    presenting: bool,
    resumed: bool,
}

#[derive(Debug, Default)]
pub struct PresentationGate {
    state: Mutex<PresentationState>,
    changed: Condvar,
}

impl PresentationGate {
    fn pause(&self) {
        let mut state = self.state.lock().unwrap();
        state.paused = true;

        drop(
            self.changed
                .wait_while(state, |state| state.presenting)
                .unwrap(),
        );
    }

    fn resume(&self) {
        let mut state = self.state.lock().unwrap();
        state.paused = false;
        state.resumed = true;

        self.changed.notify_all();
    }

    pub fn begin_frame(&self) -> bool {
        let mut state = self
            .changed
            .wait_while(self.state.lock().unwrap(), |state| state.paused)
            .unwrap();
        state.presenting = true;

        std::mem::take(&mut state.resumed)
    }

    pub fn end_frame(&self) {
        self.state.lock().unwrap().presenting = false;

        self.changed.notify_all();
    }
}
