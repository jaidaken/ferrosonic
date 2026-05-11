//! PipeWire sample rate control

use std::process::Command;
use tokio::process::Command as AsyncCommand;
use tracing::{debug, error, info};

use crate::error::AudioError;

async fn run_pw_metadata(args: &[&str]) -> Result<std::process::Output, AudioError> {
    AsyncCommand::new("pw-metadata")
        .args(args)
        .output()
        .await
        .map_err(|e| AudioError::PipeWire(format!("Failed to run pw-metadata: {}", e)))
}

pub struct PipeWireController {
    original_rate: Option<u32>,
    current_rate: Option<u32>,
}

impl PipeWireController {
    pub fn new() -> Self {
        let original_rate = Self::get_current_rate_internal().ok();
        debug!("Original PipeWire sample rate: {:?}", original_rate);

        Self {
            original_rate,
            current_rate: None,
        }
    }

    fn get_current_rate_internal() -> Result<u32, AudioError> {
        let output = Command::new("pw-metadata")
            .arg("-n")
            .arg("settings")
            .arg("0")
            .arg("clock.force-rate")
            .output()
            .map_err(|e| AudioError::PipeWire(format!("Failed to run pw-metadata: {}", e)))?;
        let stdout = String::from_utf8_lossy(&output.stdout);
        Ok(parse_force_rate_from_output(&stdout))
    }

    pub fn get_current_rate(&self) -> Option<u32> {
        self.current_rate
    }

    pub async fn set_rate(&mut self, rate: u32) -> Result<(), AudioError> {
        if self.current_rate == Some(rate) {
            debug!("Sample rate already set to {}", rate);
            return Ok(());
        }

        info!("Setting PipeWire sample rate to {} Hz", rate);
        let rate_str = rate.to_string();
        let output =
            run_pw_metadata(&["-n", "settings", "0", "clock.force-rate", &rate_str]).await?;

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

    /// Blocking shell-out from Drop — process is exiting.
    fn restore_original_blocking(&mut self) -> Result<(), AudioError> {
        if let Some(rate) = self.original_rate {
            if rate > 0 {
                info!("Restoring original sample rate: {} Hz", rate);
                let rate_str = rate.to_string();
                let output = Command::new("pw-metadata")
                    .args(["-n", "settings", "0", "clock.force-rate", &rate_str])
                    .output()
                    .map_err(|e| {
                        AudioError::PipeWire(format!("Failed to run pw-metadata: {}", e))
                    })?;
                if !output.status.success() {
                    let stderr = String::from_utf8_lossy(&output.stderr);
                    return Err(AudioError::PipeWire(format!(
                        "pw-metadata failed: {}",
                        stderr
                    )));
                }
            } else {
                info!("Clearing forced sample rate");
                let output = Command::new("pw-metadata")
                    .args(["-n", "settings", "0", "clock.force-rate", "0"])
                    .output()
                    .map_err(|e| {
                        AudioError::PipeWire(format!("Failed to run pw-metadata: {}", e))
                    })?;
                if !output.status.success() {
                    let stderr = String::from_utf8_lossy(&output.stderr);
                    return Err(AudioError::PipeWire(format!(
                        "pw-metadata failed: {}",
                        stderr
                    )));
                }
            }
        }
        Ok(())
    }

    pub async fn clear_forced_rate(&mut self) -> Result<(), AudioError> {
        info!("Clearing PipeWire forced sample rate");
        let output = run_pw_metadata(&["-n", "settings", "0", "clock.force-rate", "0"]).await?;
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
        if let Err(e) = self.restore_original_blocking() {
            error!("Failed to restore sample rate: {}", e);
        }
    }
}
