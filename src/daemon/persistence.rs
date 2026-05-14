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
        let bytes = match std::fs::read(&path) {
            Ok(b) => b,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return None,
            Err(e) => {
                tracing::warn!("queue snapshot read failed: {}", e);
                return None;
            }
        };
        match serde_json::from_slice(&bytes) {
            Ok(s) => Some(s),
            Err(e) => {
                tracing::warn!(
                    "queue snapshot at {} is corrupt ({}); ignoring",
                    path.display(),
                    e
                );
                let stamp = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_secs())
                    .unwrap_or(0);
                let bad = path.with_extension(format!("json.bad.{}", stamp));
                if let Err(rename_err) = std::fs::rename(&path, &bad) {
                    tracing::warn!("could not preserve corrupt snapshot: {}", rename_err);
                }
                None
            }
        }
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
        #[allow(clippy::disallowed_methods)]
        // allow-direct-write: tmp half of temp+rename pattern, completed below
        std::fs::write(&tmp, body)?;
        std::fs::rename(&tmp, &path)?;
        crate::config::fsync_parent_dir(&path);
        Ok(path)
    }
}
