//! Queue persistence — save to disk on changes, restore at boot.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::subsonic::models::Child;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueueSnapshot {
    pub queue: Vec<Child>,
    pub position: Option<usize>,
}

impl QueueSnapshot {
    pub fn load() -> Option<Self> {
        let path = crate::config::paths::queue_file()?;
        let bytes = std::fs::read(&path).ok()?;
        serde_json::from_slice(&bytes).ok()
    }

    /// Atomic write via temp-file + rename. Returns the path written.
    pub fn save(&self) -> std::io::Result<PathBuf> {
        let path = crate::config::paths::queue_file()
            .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::NotFound, "no config dir"))?;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let tmp = path.with_extension("json.tmp");
        let body = serde_json::to_vec(self)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        std::fs::write(&tmp, body)?;
        std::fs::rename(&tmp, &path)?;
        Ok(path)
    }
}
