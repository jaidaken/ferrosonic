//! Filesystem helpers shared across config and daemon layers.

use std::path::Path;

/// Best-effort parent-dir fsync after atomic rename so directory entry survives power loss on writeback filesystems. Silent on error since the rename itself succeeded.
///
/// ```
/// use ferrosonic::io_util::{atomic_write_bytes, fsync_parent_dir};
/// let dir = tempfile::tempdir().unwrap();
/// let p = dir.path().join("a.txt");
/// atomic_write_bytes(&p, b"x").unwrap();
/// fsync_parent_dir(&p);
/// assert!(p.exists());
/// ```
pub fn fsync_parent_dir(path: &Path) {
    if let Some(parent) = path.parent() {
        if let Ok(dir) = std::fs::File::open(parent) {
            let _ = dir.sync_all();
        }
    }
}

/// Atomic bytes-to-file via temp + fsync + rename + parent-dir fsync. Single audited entry point for the temp+rename pattern; callers using this avoid the disallowed_methods lint by routing through here.
///
/// ```
/// use ferrosonic::io_util::atomic_write_bytes;
/// let dir = tempfile::tempdir().unwrap();
/// let p = dir.path().join("x.toml");
/// atomic_write_bytes(&p, b"hello").unwrap();
/// assert_eq!(std::fs::read(&p).unwrap(), b"hello");
/// ```
pub fn atomic_write_bytes(path: &Path, body: &[u8]) -> std::io::Result<()> {
    use std::io::Write;
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() && !parent.exists() {
            std::fs::create_dir_all(parent)?;
        }
    }
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("dat");
    let tmp = path.with_extension(format!("{}.tmp", ext));
    let mut f = std::fs::OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(true)
        .open(&tmp)?;
    f.write_all(body)?;
    f.sync_all()?;
    drop(f);
    std::fs::rename(&tmp, path)?;
    fsync_parent_dir(path);
    Ok(())
}

#[cfg(test)]
mod atomic_write_bytes_smoke {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn temp_then_rename_lands_content() {
        let dir = tempdir().unwrap();
        let p = dir.path().join("x.toml");
        atomic_write_bytes(&p, b"hello").unwrap();
        assert_eq!(std::fs::read_to_string(&p).unwrap(), "hello");
    }
}
