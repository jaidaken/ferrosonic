//! Platform-specific configuration paths

use std::path::PathBuf;

/// Get the default config directory for ferrosonic
pub fn config_dir() -> Option<PathBuf> {
    dirs::config_dir().map(|p| p.join("ferrosonic"))
}

/// Get the default config file path
pub fn config_file() -> Option<PathBuf> {
    config_dir().map(|p| p.join("config.toml"))
}

/// Get the themes directory path
pub fn themes_dir() -> Option<PathBuf> {
    config_dir().map(|p| p.join("themes"))
}

/// Get the log file path
#[allow(dead_code)]
pub fn log_file() -> Option<PathBuf> {
    config_dir().map(|p| p.join("ferrosonic.log"))
}

/// Get the MPV socket path
pub fn mpv_socket_path() -> PathBuf {
    std::env::temp_dir().join("ferrosonic-mpv.sock")
}

/// Persisted queue snapshot path — daemon writes here on queue changes
/// and restores it at boot so the queue survives daemon restarts.
pub fn queue_file() -> Option<PathBuf> {
    config_dir().map(|p| p.join("queue.json"))
}

/// Ensure the config directory exists
#[allow(dead_code)]
pub fn ensure_config_dir() -> std::io::Result<PathBuf> {
    let dir = config_dir().ok_or_else(|| {
        std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "Could not determine config directory",
        )
    })?;

    if !dir.exists() {
        std::fs::create_dir_all(&dir)?;
    }

    Ok(dir)
}
