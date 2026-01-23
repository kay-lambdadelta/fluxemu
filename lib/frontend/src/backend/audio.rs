use fluxemu_runtime::{machine::Machine, platform::Platform};
use std::{fmt::Debug, sync::Arc};

/// Audio runtime to provide the frontend
pub trait AudioRuntime<P: Platform>: Sized + Debug {
    /// Create a new audio runtime
    fn new() -> Result<Self, Box<dyn std::error::Error>>;
    /// Pause audio playback
    fn pause(&mut self);
    /// Play audio
    fn play(&mut self);
    /// Set current machine
    fn set_machine(&mut self, machine: Option<Arc<Machine>>);
}
