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

/// PipeWire sample rate controller
pub struct PipeWireController {
    /// Original sample rate before ferrosonic started
    original_rate: Option<u32>,
    /// Current forced sample rate
    current_rate: Option<u32>,
}

impl PipeWireController {
    /// Create a new PipeWire controller
    pub fn new() -> Self {
        let original_rate = Self::get_current_rate_internal().ok();
        debug!("Original PipeWire sample rate: {:?}", original_rate);

        Self {
            original_rate,
            current_rate: None,
        }
    }

    /// Get current sample rate from PipeWire
    fn get_current_rate_internal() -> Result<u32, AudioError> {
        let output = Command::new("pw-metadata")
            .arg("-n")
            .arg("settings")
            .arg("0")
            .arg("clock.force-rate")
            .output()
            .map_err(|e| AudioError::PipeWire(format!("Failed to run pw-metadata: {}", e)))?;

        let stdout = String::from_utf8_lossy(&output.stdout);

        // Parse output like: "update: id:0 key:'clock.force-rate' value:'48000' type:''"
        for line in stdout.lines() {
            if line.contains("clock.force-rate") && line.contains("value:") {
                if let Some(start) = line.find("value:'") {
                    let rest = &line[start + 7..];
                    if let Some(end) = rest.find('\'') {
                        let rate_str = &rest[..end];
                        if let Ok(rate) = rate_str.parse::<u32>() {
                            return Ok(rate);
                        }
                    }
                }
            }
        }

        // No forced rate, return default
        Ok(0)
    }

    /// Get the current forced sample rate
    pub fn get_current_rate(&self) -> Option<u32> {
        self.current_rate
    }

    /// Set the sample rate
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

    /// Restore original sample rate (best-effort sync version used from Drop).
    /// Process is exiting, so a brief blocking shell-out is acceptable.
    fn restore_original_blocking(&mut self) -> Result<(), AudioError> {
        if let Some(rate) = self.original_rate {
            if rate > 0 {
                info!("Restoring original sample rate: {} Hz", rate);
                let rate_str = rate.to_string();
                let output = Command::new("pw-metadata")
                    .args(["-n", "settings", "0", "clock.force-rate", &rate_str])
                    .output()
                    .map_err(|e| AudioError::PipeWire(format!("Failed to run pw-metadata: {}", e)))?;
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
                    .map_err(|e| AudioError::PipeWire(format!("Failed to run pw-metadata: {}", e)))?;
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

    /// Clear the forced sample rate (let PipeWire use default)
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

