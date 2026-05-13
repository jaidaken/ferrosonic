//! PipeWire sample rate control

use std::process::{Command, Output};
use std::sync::Arc;

use async_trait::async_trait;
use tokio::process::Command as AsyncCommand;
use tracing::{debug, error, info};

use crate::error::AudioError;

/// Injectable runner for the `pw-metadata` shell-out. Production
/// uses `PwMetadataCommand`; tests use a fake that returns scripted output.
#[async_trait]
pub trait CommandRunner: Send + Sync {
    async fn run(&self, args: &[&str]) -> Result<Output, AudioError>;
    fn run_blocking(&self, args: &[&str]) -> Result<Output, AudioError>;
}

pub struct PwMetadataCommand;

#[async_trait]
impl CommandRunner for PwMetadataCommand {
    async fn run(&self, args: &[&str]) -> Result<Output, AudioError> {
        AsyncCommand::new("pw-metadata")
            .args(args)
            .output()
            .await
            .map_err(|e| AudioError::PipeWire(format!("Failed to run pw-metadata: {}", e)))
    }

    fn run_blocking(&self, args: &[&str]) -> Result<Output, AudioError> {
        Command::new("pw-metadata")
            .args(args)
            .output()
            .map_err(|e| AudioError::PipeWire(format!("Failed to run pw-metadata: {}", e)))
    }
}

pub struct PipeWireController {
    original_rate: Option<u32>,
    current_rate: Option<u32>,
    runner: Arc<dyn CommandRunner>,
}

impl PipeWireController {
    pub fn new() -> Self {
        Self::with_runner(Arc::new(PwMetadataCommand))
    }

    pub fn with_runner(runner: Arc<dyn CommandRunner>) -> Self {
        let original_rate = Self::query_rate_via(&*runner).ok();
        debug!("Original PipeWire sample rate: {:?}", original_rate);
        Self {
            original_rate,
            current_rate: None,
            runner,
        }
    }

    fn query_rate_via(runner: &dyn CommandRunner) -> Result<u32, AudioError> {
        let output = runner.run_blocking(&["-n", "settings", "0", "clock.force-rate"])?;
        let stdout = String::from_utf8_lossy(&output.stdout);
        Ok(parse_force_rate_from_output(&stdout))
    }

    pub fn get_current_rate(&self) -> Option<u32> {
        self.current_rate
    }

    pub fn get_original_rate(&self) -> Option<u32> {
        self.original_rate
    }

    pub async fn set_rate(&mut self, rate: u32) -> Result<(), AudioError> {
        // No cache short-circuit: external pw-metadata changes would
        // make the cache stale and bit-perfect would silently break.
        info!("Setting PipeWire sample rate to {} Hz", rate);
        let rate_str = rate.to_string();
        let output = self
            .runner
            .run(&["-n", "settings", "0", "clock.force-rate", &rate_str])
            .await?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(AudioError::PipeWire(format!(
                "pw-metadata failed: {}",
                stderr
            )));
        }
        self.current_rate = Some(rate);
        Ok(())
    }


    pub async fn clear_forced_rate(&mut self) -> Result<(), AudioError> {
        info!("Clearing PipeWire forced sample rate");
        let output = self
            .runner
            .run(&["-n", "settings", "0", "clock.force-rate", "0"])
            .await?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(AudioError::PipeWire(format!(
                "pw-metadata failed: {}",
                stderr
            )));
        }
        self.current_rate = None;
        Ok(())
    }
}

/// Parses `value:'<rate>'` from pw-metadata output. Returns 0 if absent.
pub fn parse_force_rate_from_output(stdout: &str) -> u32 {
    for line in stdout.lines() {
        if line.contains("clock.force-rate") && line.contains("value:") {
            if let Some(start) = line.find("value:'") {
                let rest = &line[start + 7..];
                if let Some(end) = rest.find('\'') {
                    if let Ok(rate) = rest[..end].parse::<u32>() {
                        return rate;
                    }
                }
            }
        }
    }
    0
}

impl Default for PipeWireController {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for PipeWireController {
    fn drop(&mut self) {
        // Spawn a worker thread with a 3s join timeout so a hung
        // pw-metadata (pipewire daemon dead) can't block process exit.
        let original_rate = self.original_rate;
        let runner = self.runner.clone();
        let handle = std::thread::spawn(move || {
            if let Some(rate) = original_rate {
                let rate_str = if rate > 0 {
                    rate.to_string()
                } else {
                    "0".to_string()
                };
                let _ = runner.run_blocking(&[
                    "-n",
                    "settings",
                    "0",
                    "clock.force-rate",
                    &rate_str,
                ]);
            }
        });
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
        while !handle.is_finished() && std::time::Instant::now() < deadline {
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        if !handle.is_finished() {
            error!("PipeWire restore-on-drop timed out; abandoning worker thread");
        }
    }
}
