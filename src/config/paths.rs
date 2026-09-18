use std::path::PathBuf;

/// Honors `FERROSONIC_CONFIG_DIR` for tests; XDG otherwise.
#[must_use]
pub fn config_dir() -> Option<PathBuf> {
    if let Some(override_path) = std::env::var_os("FERROSONIC_CONFIG_DIR") {
        return Some(PathBuf::from(override_path));
    }
    dirs::config_dir().map(|p| p.join("ferrosonic"))
}

/// Path of `config.toml` under the XDG config dir.
#[must_use]
pub fn config_file() -> Option<PathBuf> {
    config_dir().map(|p| p.join("config.toml"))
}

/// Path of the user themes directory.
#[must_use]
pub fn themes_dir() -> Option<PathBuf> {
    config_dir().map(|p| p.join("themes"))
}

/// Path of the daemon log file.
#[must_use]
pub fn log_file() -> Option<PathBuf> {
    config_dir().map(|p| p.join("ferrosonic.log"))
}

/// Path of the mpv IPC socket under the runtime dir.
#[must_use]
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
    std::env::temp_dir().join(format!("ferrosonic-mpv-{uid}.sock"))
}

/// Path of the persisted queue snapshot.
#[must_use]
pub fn queue_file() -> Option<PathBuf> {
    config_dir().map(|p| p.join("queue.json"))
}

/// Path of the persisted recent-search history.
#[must_use]
pub fn search_history_file() -> Option<PathBuf> {
    config_dir().map(|p| p.join("search_history.json"))
}

/// Root cache directory for on-disk artifacts. Honors `FERROSONIC_CACHE_DIR`
/// for tests; XDG cache otherwise.
#[must_use]
pub fn cache_dir() -> Option<PathBuf> {
    if let Some(override_path) = std::env::var_os("FERROSONIC_CACHE_DIR") {
        return Some(PathBuf::from(override_path));
    }
    dirs::cache_dir().map(|p| p.join("ferrosonic"))
}

/// Directory holding cached track audio.
#[must_use]
pub fn tracks_dir() -> Option<PathBuf> {
    cache_dir().map(|p| p.join("tracks"))
}

/// Create the config directory if missing and return it.
///
/// # Errors
/// Returns an error if the config directory cannot be created.
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
    // Owner-only: the directory holds the credential config and logs.
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700));
    }

    Ok(dir)
}
