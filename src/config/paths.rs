use std::path::PathBuf;

/// Honors FERROSONIC_CONFIG_DIR for tests; XDG otherwise.
pub fn config_dir() -> Option<PathBuf> {
    if let Some(override_path) = std::env::var_os("FERROSONIC_CONFIG_DIR") {
        return Some(PathBuf::from(override_path));
    }
    dirs::config_dir().map(|p| p.join("ferrosonic"))
}

pub fn config_file() -> Option<PathBuf> {
    config_dir().map(|p| p.join("config.toml"))
}

pub fn themes_dir() -> Option<PathBuf> {
    config_dir().map(|p| p.join("themes"))
}

#[allow(dead_code)]
pub fn log_file() -> Option<PathBuf> {
    config_dir().map(|p| p.join("ferrosonic.log"))
}

pub fn mpv_socket_path() -> PathBuf {
    std::env::temp_dir().join("ferrosonic-mpv.sock")
}

pub fn queue_file() -> Option<PathBuf> {
    config_dir().map(|p| p.join("queue.json"))
}

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
