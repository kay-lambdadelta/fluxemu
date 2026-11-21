use std::{
    fs::File,
    io::Read,
    mem::MaybeUninit,
    slice,
    sync::mpsc::{self, Receiver},
    thread,
};

use egui::{Event, Modifiers, PointerButton, Pos2, TouchDeviceId, TouchId, TouchPhase};
use nalgebra::Point2;

use crate::sys;

// NOTE: This was only tested on a single board with an extremely hacky rube goldberg-esque setup
//
// It probably needs more work and more testing

const DEVICE_PATH: &str = "/dev/input0";

pub struct Touchscreen {
    points: Receiver<sys::touch_point_s>,
    last_position: Point2<f32>,
    pressed: bool,
}

impl Touchscreen {
    pub fn open() -> Option<Self> {
        let mut file = File::open(DEVICE_PATH).ok()?;

        let (sender, points) = mpsc::channel();

        thread::Builder::new()
            .name("touchscreen poll".to_string())
            .spawn(move || {
                while let Some(point) = read_point(&mut file) {
                    if sender.send(point).is_err() {
                        break;
                    }
                }
            })
            .inspect_err(|error| tracing::error!("Could not start touchscreen thread: {error}"))
            .ok()?;

        Some(Self {
            points,
            last_position: Point2::origin(),
            pressed: false,
        })
    }

    pub fn poll(&mut self, events: &mut Vec<Event>) {
        while let Ok(point) = self.points.try_recv() {
            let flags = u32::from(point.flags);

            if flags & (sys::TOUCH_DOWN | sys::TOUCH_MOVE | sys::TOUCH_UP) == 0 {
                continue;
            }

            if flags & sys::TOUCH_POS_VALID != 0 {
                self.last_position = Point2::new(f32::from(point.x), f32::from(point.y));
            }

            let id = TouchId(point.id.into());
            let started = !self.pressed;

            if started {
                self.push(events, id, TouchPhase::Start);
            }

            if flags & sys::TOUCH_UP != 0 {
                self.push(events, id, TouchPhase::End);
            } else if !started {
                self.push(events, id, TouchPhase::Move);
            }
        }
    }

    fn push(&mut self, events: &mut Vec<Event>, id: TouchId, phase: TouchPhase) {
        let position = Pos2::new(self.last_position.x, self.last_position.y);

        events.push(Event::Touch {
            device_id: TouchDeviceId(0),
            id,
            phase,
            pos: position,
            force: None,
        });

        match phase {
            TouchPhase::Start => {
                self.pressed = true;
                events.push(Event::PointerMoved(position));
                events.push(button_event(position, true));
            }
            TouchPhase::Move => events.push(Event::PointerMoved(position)),
            TouchPhase::End | TouchPhase::Cancel => {
                self.pressed = false;
                events.push(button_event(position, false));
                events.push(Event::PointerGone);
            }
        }
    }
}

fn read_point(file: &mut File) -> Option<sys::touch_point_s> {
    loop {
        let mut sample = MaybeUninit::<sys::touch_sample_s>::zeroed();
        let buffer = unsafe {
            slice::from_raw_parts_mut(
                sample.as_mut_ptr().cast::<u8>(),
                size_of::<sys::touch_sample_s>(),
            )
        };

        match file.read(buffer) {
            Ok(length) if length == buffer.len() => {}
            Ok(length) => {
                tracing::warn!("Short touchscreen read of {length} bytes");
                return None;
            }
            Err(error) => {
                tracing::error!("Could not read touchscreen: {error}");
                return None;
            }
        }

        let sample = unsafe { sample.assume_init() };

        if sample.npoints > 0 {
            return Some(sample.point[0]);
        }
    }
}

fn button_event(position: Pos2, pressed: bool) -> Event {
    Event::PointerButton {
        pos: position,
        button: PointerButton::Primary,
        pressed,
        modifiers: Modifiers::default(),
    }
}
