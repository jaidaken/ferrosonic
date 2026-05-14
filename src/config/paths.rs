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

pub fn log_file() -> Option<PathBuf> {
    config_dir().map(|p| p.join("ferrosonic.log"))
}

pub fn mpv_socket_path() -> PathBuf {
    // Prefer $XDG_RUNTIME_DIR (per-user, mode 0700) when present;
    // otherwise UID-scope the /tmp path so two users on the same host
    // do not collide on the shared socket.
    if let Some(rt) = std::env::var_os("XDG_RUNTIME_DIR") {
        let rt = PathBuf::from(rt);
        if rt.exists() {
            return rt.join("ferrosonic-mpv.sock");
        }
    }
    let uid = unsafe { libc::getuid() };
    std::env::temp_dir().join(format!("ferrosonic-mpv-{}.sock", uid))
}

pub fn queue_file() -> Option<PathBuf> {
    config_dir().map(|p| p.join("queue.json"))
}

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
