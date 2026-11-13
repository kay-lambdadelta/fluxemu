use std::time::{Duration, Instant};

use fluxemu_frontend::simulation_controller::{Controller, JITTER_CEILING};
use wasm_bindgen::JsValue;
use web_sys::{Document, Element};

const OVERLAY_ID: &str = "debug-overlay";
const UPDATE_RATE: Duration = Duration::from_millis(200);

pub struct DebugOverlay {
    overlay: Element,
    execution_timeslice: Element,
    target_timeslice: Element,
    hardware_speed: Element,
    jitter_ratio: Element,
    visible: bool,
    last_update: Option<Instant>,
}

impl DebugOverlay {
    pub fn new() -> Result<Self, JsValue> {
        let document = web_sys::window()
            .and_then(|window| window.document())
            .ok_or("This environment has no document")?;

        Ok(Self {
            overlay: get_element(&document, OVERLAY_ID)?,
            execution_timeslice: get_element(&document, "debug-overlay-execution-timeslice")?,
            target_timeslice: get_element(&document, "debug-overlay-target-timeslice")?,
            hardware_speed: get_element(&document, "debug-overlay-hardware-speed")?,
            jitter_ratio: get_element(&document, "debug-overlay-jitter-ratio")?,
            visible: false,
            last_update: None,
        })
    }

    pub fn toggle(&mut self) {
        self.visible = !self.visible;
        self.last_update = None;
    }

    pub fn update(&mut self, controller: Option<&Controller>) {
        let controller = controller.filter(|_| self.visible);
        let _ = self
            .overlay
            .toggle_attribute_with_force("hidden", controller.is_none());

        let Some(controller) = controller else {
            return;
        };

        if self
            .last_update
            .is_some_and(|last_update| last_update.elapsed() < UPDATE_RATE)
        {
            return;
        }

        let state = controller.get_state_snapshot();

        self.execution_timeslice.set_text_content(Some(&format!(
            "{:.2?}",
            Duration::from_secs_f32(state.execution_timeslice)
        )));
        self.target_timeslice.set_text_content(Some(&format!(
            "{:.2?}",
            Duration::from_secs_f32(state.target_timeslice)
        )));
        self.hardware_speed
            .set_text_content(Some(&format!("{:.1}%", state.hardware_speed_ema * 100.0)));
        self.jitter_ratio.set_text_content(Some(&format!(
            "{:.1}% (ceiling {:.1}%)",
            state.jitter_ratio * 100.0,
            JITTER_CEILING * 100.0
        )));

        self.last_update = Some(Instant::now());
    }
}

fn get_element(document: &Document, id: &str) -> Result<Element, JsValue> {
    document
        .get_element_by_id(id)
        .ok_or_else(|| JsValue::from_str(&format!("This page has no #{id} element")))
}
