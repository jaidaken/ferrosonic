//! Recording `pw-metadata` runner so tests can assert the PipeWire
//! force-rate pin is set on play and cleared on pause/stop without a
//! real PipeWire daemon.

use std::os::unix::process::ExitStatusExt;
use std::process::{ExitStatus, Output};
use std::sync::{Arc, Mutex};

use async_trait::async_trait;

use ferrosonic::audio::pipewire::CommandRunner;
use ferrosonic::error::AudioError;

#[derive(Clone, Default)]
pub struct RecordingPwRunner {
    calls: Arc<Mutex<Vec<Vec<String>>>>,
}

impl RecordingPwRunner {
    pub fn new() -> Self {
        Self::default()
    }

    fn record(&self, args: &[&str]) {
        self.calls
            .lock()
            .expect("pw recorder lock")
            .push(args.iter().map(|s| (*s).to_string()).collect());
    }

    /// Values passed to `clock.force-rate`, in call order. `set_rate`
    /// records the rate string; `clear_forced_rate` records `"0"`. The
    /// read-only probe (no value arg) is excluded.
    pub fn force_rate_values(&self) -> Vec<String> {
        self.calls
            .lock()
            .expect("pw recorder lock")
            .iter()
            .filter_map(|c| match c.as_slice() {
                [_, _, _, key, value] if key == "clock.force-rate" => Some(value.clone()),
                _ => None,
            })
            .collect()
    }
}

#[async_trait]
impl CommandRunner for RecordingPwRunner {
    async fn run(&self, args: &[&str]) -> Result<Output, AudioError> {
        self.record(args);
        Ok(ok_output())
    }

    fn run_blocking(&self, args: &[&str]) -> Result<Output, AudioError> {
        self.record(args);
        Ok(ok_output())
    }
}

fn ok_output() -> Output {
    Output {
        status: ExitStatus::from_raw(0),
        stdout: Vec::new(),
        stderr: Vec::new(),
    }
}
