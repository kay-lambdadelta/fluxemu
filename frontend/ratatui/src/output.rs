use std::{
    io::{self, Write, stdout},
    mem,
    sync::{Arc, Condvar, Mutex},
    thread::JoinHandle,
    time::{Duration, Instant},
};

use crate::external_input::{ExternalInputSender, Message};

#[derive(Debug, Default, Clone, Copy)]
pub struct OutputTotals {
    pub bytes: u64,
    pub write_time: Duration,
}

#[derive(Debug, Default)]
struct OutputState {
    pending: Vec<u8>,
    writing: bool,
    closed: bool,
    totals: OutputTotals,
}

#[derive(Debug, Default)]
struct SharedOutput {
    state: Mutex<OutputState>,
    wake: Condvar,
}

#[derive(Debug, Clone)]
pub struct OutputMonitor(Arc<SharedOutput>);

impl OutputMonitor {
    pub fn is_idle(&self) -> bool {
        let state = self.0.state.lock().unwrap();

        state.pending.is_empty() && !state.writing
    }
}

#[derive(Debug)]
pub struct TerminalOutput {
    staged: Vec<u8>,
    shared: Arc<SharedOutput>,
    writer: Option<JoinHandle<()>>,
}

impl TerminalOutput {
    pub fn new(sender: ExternalInputSender) -> io::Result<Self> {
        let shared = Arc::new(SharedOutput::default());
        let writer = std::thread::Builder::new()
            .name("Terminal Output".to_string())
            .spawn({
                let shared = shared.clone();

                move || write_pending(&shared, &sender)
            })?;

        Ok(Self {
            staged: Vec::default(),
            shared,
            writer: Some(writer),
        })
    }

    pub fn monitor(&self) -> OutputMonitor {
        OutputMonitor(self.shared.clone())
    }
}

impl Write for TerminalOutput {
    fn write(&mut self, buffer: &[u8]) -> io::Result<usize> {
        self.staged.extend_from_slice(buffer);

        Ok(buffer.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        if !self.staged.is_empty() {
            let mut state = self.shared.state.lock().unwrap();

            state.pending.append(&mut self.staged);
            self.shared.wake.notify_one();
        }

        Ok(())
    }
}

impl Drop for TerminalOutput {
    fn drop(&mut self) {
        let _ = self.flush();

        self.shared.state.lock().unwrap().closed = true;
        self.shared.wake.notify_one();

        if let Some(writer) = self.writer.take() {
            let _ = writer.join();
        }
    }
}

fn write_pending(shared: &SharedOutput, sender: &ExternalInputSender) {
    let mut output = stdout();

    loop {
        let bytes = {
            let state = shared.state.lock().unwrap();
            let mut state = shared
                .wake
                .wait_while(state, |state| state.pending.is_empty() && !state.closed)
                .unwrap();

            if state.pending.is_empty() {
                return;
            }

            state.writing = true;
            mem::take(&mut state.pending)
        };

        let start = Instant::now();
        let result = output.write_all(&bytes).and_then(|()| output.flush());
        let write_time = start.elapsed();

        let is_idle = {
            let mut state = shared.state.lock().unwrap();

            state.writing = false;
            state.totals.bytes += bytes.len() as u64;
            state.totals.write_time += write_time;
            state.pending.is_empty()
        };

        match result {
            Ok(()) if is_idle => sender.send(Message::OutputIdle),
            Ok(()) => {}
            Err(err) => {
                sender.send(Message::TerminalError(err));
                return;
            }
        }
    }
}
